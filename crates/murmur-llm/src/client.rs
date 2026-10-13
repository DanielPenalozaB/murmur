//! Streaming HTTP clients. Rust has no official Anthropic or Gemini SDK, so these speak
//! the REST APIs directly: Anthropic Messages (SSE), Gemini `streamGenerateContent`
//! (SSE) and Ollama `/api/chat` (newline-delimited JSON).

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{LlmError, Prompt, Provider};

const ANTHROPIC_URL: &str = "https://api.anthropic.com";
const GEMINI_URL: &str = "https://generativelanguage.googleapis.com";
/// How long Ollama keeps the model in memory after a request. Its default (5 min)
/// unloads it during quiet stretches of a meeting, and reloading takes 10–20 s.
const OLLAMA_KEEP_ALIVE: &str = "30m";

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
}

/// Streams the answer to `prompt`, calling `on_text` with each piece as it arrives.
/// Returns the full text. `api_key` is required for cloud providers.
pub async fn stream(
    provider: &Provider,
    api_key: Option<&str>,
    prompt: &Prompt,
    mut on_text: impl FnMut(&str),
) -> Result<String, LlmError> {
    let client = reqwest::Client::new();
    let (request, format) = match provider {
        Provider::Ollama {
            base_url,
            model,
            num_ctx,
        } => {
            let body = json!({
                "model": model,
                "stream": true,
                // Qwen3 and other reasoning models think first by default, which delays
                // the first word by many seconds. Notes and answers don't need it.
                "think": false,
                "keep_alive": OLLAMA_KEEP_ALIVE,
                "options": { "num_ctx": num_ctx },
                "messages": [
                    { "role": "system", "content": prompt.system },
                    { "role": "user", "content": format!("{}\n\n{}", prompt.transcript_block(), prompt.instruction) },
                ],
            });
            (
                client.post(format!("{base_url}/api/chat")).json(&body),
                Format::Ndjson,
            )
        }
        Provider::Anthropic { model } => {
            let key = api_key.ok_or(LlmError::MissingKey("Claude"))?;
            let body = json!({
                "model": model,
                "max_tokens": prompt.max_tokens,
                "stream": true,
                "system": prompt.system,
                "messages": [{
                    "role": "user",
                    "content": [
                        // The transcript only grows, so the cached prefix is reused by the
                        // next question and only new lines are billed at the full rate.
                        { "type": "text", "text": prompt.transcript_block(), "cache_control": { "type": "ephemeral" } },
                        { "type": "text", "text": prompt.instruction },
                    ],
                }],
            });
            let request = client
                .post(format!("{ANTHROPIC_URL}/v1/messages"))
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .json(&body);
            (request, Format::Sse)
        }
        Provider::Gemini { model } => {
            let key = api_key.ok_or(LlmError::MissingKey("Gemini"))?;
            let body = json!({
                "systemInstruction": { "parts": [{ "text": prompt.system }] },
                "contents": [{
                    "role": "user",
                    "parts": [{ "text": prompt.transcript_block() }, { "text": prompt.instruction }],
                }],
                "generationConfig": { "maxOutputTokens": prompt.max_tokens },
            });
            let request = client
                .post(format!(
                    "{GEMINI_URL}/v1beta/models/{model}:streamGenerateContent?alt=sse"
                ))
                .header("x-goog-api-key", key)
                .json(&body);
            (request, Format::Sse)
        }
    };

    let response = request.send().await.map_err(|e| connection_error(provider, e))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(LlmError::Api(api_error_message(provider, status.as_u16(), &body)));
    }

    let mut full = String::new();
    let mut buffer = Vec::new();
    let mut bytes = response.bytes_stream();
    while let Some(chunk) = bytes.next().await {
        let chunk = chunk.map_err(|e| LlmError::Http(e.to_string()))?;
        buffer.extend_from_slice(&chunk);
        // Handle complete lines only; a JSON event can span network chunks.
        while let Some(newline) = buffer.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buffer.drain(..=newline).collect();
            let line = String::from_utf8_lossy(&line);
            let payload = match format {
                Format::Ndjson => line.trim(),
                Format::Sse => match line.trim().strip_prefix("data:") {
                    Some(data) => data.trim(),
                    None => continue,
                },
            };
            if payload.is_empty() {
                continue;
            }
            let Ok(event) = serde_json::from_str::<Value>(payload) else {
                continue;
            };
            match parse_event(provider, &event)? {
                Event::Text(text) => {
                    on_text(&text);
                    full.push_str(&text);
                }
                Event::Done => return Ok(full),
                Event::Skip => {}
            }
        }
    }
    Ok(full)
}

/// Loads a local model into memory ahead of the first question, so it answers right
/// away. Does nothing for cloud providers. The context size must match later requests,
/// otherwise Ollama reloads the model.
pub async fn warm_up(provider: &Provider) -> Result<(), LlmError> {
    let Provider::Ollama {
        base_url,
        model,
        num_ctx,
    } = provider
    else {
        return Ok(());
    };
    // A generate request without a prompt only loads the model.
    let body = json!({
        "model": model,
        "keep_alive": OLLAMA_KEEP_ALIVE,
        "options": { "num_ctx": num_ctx },
    });
    let response = reqwest::Client::new()
        .post(format!("{base_url}/api/generate"))
        .json(&body)
        .send()
        .await
        .map_err(|e| connection_error(provider, e))?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(LlmError::Api(api_error_message(provider, status, &body)));
    }
    Ok(())
}

/// Models the user can pick for a provider. Cloud providers need the key.
pub async fn list_models(provider: &Provider, api_key: Option<&str>) -> Result<Vec<ModelInfo>, LlmError> {
    let client = reqwest::Client::new();
    let request = match provider {
        Provider::Ollama { base_url, .. } => client.get(format!("{base_url}/api/tags")),
        Provider::Anthropic { .. } => client
            .get(format!("{ANTHROPIC_URL}/v1/models?limit=100"))
            .header("x-api-key", api_key.ok_or(LlmError::MissingKey("Claude"))?)
            .header("anthropic-version", "2023-06-01"),
        Provider::Gemini { .. } => client
            .get(format!("{GEMINI_URL}/v1beta/models?pageSize=1000"))
            .header("x-goog-api-key", api_key.ok_or(LlmError::MissingKey("Gemini"))?),
    };
    let response = request.send().await.map_err(|e| connection_error(provider, e))?;
    let status = response.status();
    let body = response.text().await.map_err(|e| LlmError::Http(e.to_string()))?;
    if !status.is_success() {
        return Err(LlmError::Api(api_error_message(provider, status.as_u16(), &body)));
    }
    let json: Value = serde_json::from_str(&body).map_err(|e| LlmError::Http(e.to_string()))?;
    Ok(parse_models(provider, &json))
}

#[derive(Clone, Copy)]
enum Format {
    /// Server-sent events: `data: {json}` lines.
    Sse,
    /// One JSON object per line.
    Ndjson,
}

#[derive(Debug, PartialEq)]
enum Event {
    Text(String),
    Done,
    Skip,
}

fn parse_event(provider: &Provider, event: &Value) -> Result<Event, LlmError> {
    let str_at = |pointer: &str| event.pointer(pointer).and_then(Value::as_str);
    match provider {
        Provider::Ollama { .. } => {
            if let Some(error) = str_at("/error") {
                return Err(LlmError::Api(error.to_string()));
            }
            if let Some(text) = str_at("/message/content").filter(|t| !t.is_empty()) {
                return Ok(Event::Text(text.to_string()));
            }
            if event.get("done").and_then(Value::as_bool) == Some(true) {
                return Ok(Event::Done);
            }
            Ok(Event::Skip)
        }
        Provider::Anthropic { .. } => match str_at("/type") {
            // Thinking deltas are skipped; only the visible answer is streamed.
            Some("content_block_delta") if str_at("/delta/type") == Some("text_delta") => {
                Ok(Event::Text(str_at("/delta/text").unwrap_or_default().to_string()))
            }
            Some("message_delta") if str_at("/delta/stop_reason") == Some("refusal") => {
                Err(LlmError::Refused)
            }
            Some("message_stop") => Ok(Event::Done),
            Some("error") => Err(LlmError::Api(
                str_at("/error/message").unwrap_or("unknown error").to_string(),
            )),
            _ => Ok(Event::Skip),
        },
        Provider::Gemini { .. } => {
            if let Some(error) = str_at("/error/message") {
                return Err(LlmError::Api(error.to_string()));
            }
            let text: String = event
                .pointer("/candidates/0/content/parts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                // Parts flagged as thoughts are the model's reasoning, not the answer.
                .filter(|p| p.get("thought").and_then(Value::as_bool) != Some(true))
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect();
            if text.is_empty() {
                Ok(Event::Skip)
            } else {
                Ok(Event::Text(text))
            }
        }
    }
}

fn parse_models(provider: &Provider, json: &Value) -> Vec<ModelInfo> {
    let items = |key: &str| {
        json.get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let str_of = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
    match provider {
        Provider::Ollama { .. } => items("models")
            .iter()
            .filter_map(|m| str_of(m, "name"))
            .map(|id| ModelInfo {
                label: id.clone(),
                id,
            })
            .collect(),
        Provider::Anthropic { .. } => items("data")
            .iter()
            .filter_map(|m| {
                let id = str_of(m, "id")?;
                Some(ModelInfo {
                    label: str_of(m, "display_name").unwrap_or_else(|| id.clone()),
                    id,
                })
            })
            .collect(),
        Provider::Gemini { .. } => items("models")
            .iter()
            .filter(|m| {
                m.get("supportedGenerationMethods")
                    .and_then(Value::as_array)
                    .is_some_and(|methods| methods.iter().any(|x| x == "generateContent"))
            })
            .filter_map(|m| {
                let id = str_of(m, "name")?.trim_start_matches("models/").to_string();
                Some(ModelInfo {
                    label: str_of(m, "displayName").unwrap_or_else(|| id.clone()),
                    id,
                })
            })
            .collect(),
    }
}

fn connection_error(provider: &Provider, e: reqwest::Error) -> LlmError {
    match provider {
        Provider::Ollama { base_url, .. } if e.is_connect() => LlmError::Http(format!(
            "Ollama isn't running at {base_url}. Open the Ollama app or run `ollama serve`."
        )),
        _ => LlmError::Http(e.to_string()),
    }
}

fn api_error_message(provider: &Provider, status: u16, body: &str) -> String {
    let json: Option<Value> = serde_json::from_str(body).ok();
    let message = json
        .as_ref()
        .and_then(|j| {
            j.pointer("/error/message")
                .or_else(|| j.get("error"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.chars().take(300).collect());
    match (provider, status) {
        (_, 401 | 403) => format!("the API key was rejected ({status}): {message}"),
        (Provider::Ollama { model, .. }, 404) => {
            format!("{message}. Download it with `ollama pull {model}` or pick another model in Settings.")
        }
        _ => format!("{status}: {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anthropic() -> Provider {
        Provider::Anthropic {
            model: "claude-opus-5-5".into(),
        }
    }

    fn gemini() -> Provider {
        Provider::Gemini { model: "m".into() }
    }

    fn parse(provider: &Provider, json: &str) -> Result<Event, LlmError> {
        parse_event(provider, &serde_json::from_str(json).unwrap())
    }

    #[test]
    fn parses_ollama_chunks() {
        let p = Provider::default();
        assert_eq!(
            parse(
                &p,
                r#"{"message":{"role":"assistant","content":"Hi"},"done":false}"#
            )
            .unwrap(),
            Event::Text("Hi".into())
        );
        assert_eq!(
            parse(&p, r#"{"message":{"role":"assistant","content":""},"done":true}"#).unwrap(),
            Event::Done
        );
        assert!(matches!(
            parse(&p, r#"{"error":"model not found"}"#),
            Err(LlmError::Api(_))
        ));
    }

    #[test]
    fn parses_anthropic_events() {
        let p = anthropic();
        assert_eq!(
            parse(
                &p,
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#
            )
            .unwrap(),
            Event::Text("Hello".into())
        );
        assert_eq!(
            parse(&p, r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"hm"}}"#)
                .unwrap(),
            Event::Skip
        );
        assert_eq!(parse(&p, r#"{"type":"message_stop"}"#).unwrap(), Event::Done);
        assert!(matches!(
            parse(
                &p,
                r#"{"type":"message_delta","delta":{"stop_reason":"refusal"}}"#
            ),
            Err(LlmError::Refused)
        ));
        assert!(matches!(
            parse(&p, r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#),
            Err(LlmError::Api(m)) if m == "Overloaded"
        ));
    }

    #[test]
    fn parses_gemini_chunks_without_thoughts() {
        let json =
            r#"{"candidates":[{"content":{"parts":[{"text":"plan","thought":true},{"text":"Answer"}]}}]}"#;
        assert_eq!(parse(&gemini(), json).unwrap(), Event::Text("Answer".into()));
    }

    #[test]
    fn lists_models_per_provider() {
        let ollama: Value = serde_json::from_str(r#"{"models":[{"name":"qwen3:14b"}]}"#).unwrap();
        assert_eq!(parse_models(&Provider::default(), &ollama)[0].id, "qwen3:14b");

        let claude: Value =
            serde_json::from_str(r#"{"data":[{"id":"claude-haiku-5-5","display_name":"Claude Haiku 5.5"}]}"#)
                .unwrap();
        assert_eq!(parse_models(&anthropic(), &claude)[0].label, "Claude Haiku 5.5");

        let g: Value = serde_json::from_str(
            r#"{"models":[
                {"name":"models/embed","supportedGenerationMethods":["embedContent"]},
                {"name":"models/flash","displayName":"Flash","supportedGenerationMethods":["generateContent"]}
            ]}"#,
        )
        .unwrap();
        let models = parse_models(&gemini(), &g);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "flash");
    }

    #[test]
    fn explains_common_api_errors() {
        let body =
            r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        assert!(api_error_message(&anthropic(), 401, body).starts_with("the API key was rejected"));
        let missing = api_error_message(
            &Provider::default(),
            404,
            r#"{"error":"model 'qwen3:14b' not found"}"#,
        );
        assert!(missing.contains("ollama pull qwen3:14b"), "{missing}");
    }
}
