//! Ask (questions about a meeting, live or saved) and notes, through the provider in
//! settings. Cloud API keys live in the macOS Keychain and never reach the webview.

use murmur_core::{ExportFormat, Segment, SpeakerNames};
use murmur_llm::{ModelInfo, Prompt, Provider};
use security_framework::passwords;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::library::LibraryState;
use crate::recording::RecordingState;
use crate::settings::SettingsState;

const KEYCHAIN_SERVICE: &str = "dev.murmur.app";
/// `errSecItemNotFound` from Security.framework.
const KEY_NOT_FOUND: i32 = -25300;

/// Streamed to the UI while the model writes.
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamEvent {
    Text { text: String },
    Done { text: String },
    Error { message: String },
}

fn api_key(provider: &Provider) -> Option<String> {
    let name = provider.key_name()?;
    let bytes = passwords::get_generic_password(KEYCHAIN_SERVICE, name).ok()?;
    String::from_utf8(bytes).ok()
}

fn provider(app: &AppHandle) -> Provider {
    app.state::<SettingsState>().0.lock().unwrap().llm.clone()
}

/// Segments and speaker names of a saved session, or of the recording in progress
/// when `id` is `None` (speakers aren't known until it stops).
fn segments(app: &AppHandle, id: Option<&str>) -> Result<(Vec<Segment>, SpeakerNames), String> {
    match id {
        None => Ok((app.state::<RecordingState>().live_segments(), SpeakerNames::new())),
        Some(id) => {
            let library = app.state::<LibraryState>();
            let index = library.0.lock().unwrap();
            let found = index.get_session(id).map_err(|e| e.to_string())?;
            let names = index.speaker_names(id).map_err(|e| e.to_string())?;
            Ok((found.ok_or("session not found")?.1, names))
        }
    }
}

/// Runs a prompt and streams it to `events`. Errors are reported as events too, so
/// the UI handles one path.
async fn run(app: &AppHandle, prompt: Prompt, events: &Channel<StreamEvent>) -> Option<String> {
    let provider = provider(app);
    let key = api_key(&provider);
    let result = murmur_llm::stream(&provider, key.as_deref(), &prompt, |text| {
        let _ = events.send(StreamEvent::Text {
            text: text.to_string(),
        });
    })
    .await;
    match result {
        Ok(text) => Some(text),
        Err(e) => {
            tracing::warn!("{} failed: {e}", provider.label());
            let _ = events.send(StreamEvent::Error {
                message: e.to_string(),
            });
            None
        }
    }
}

#[tauri::command]
pub async fn ask(
    app: AppHandle,
    session_id: Option<String>,
    question: String,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    let (segments, names) = segments(&app, session_id.as_deref())?;
    if segments.is_empty() {
        return Err("nothing has been transcribed yet".into());
    }
    if let Some(text) = run(
        &app,
        murmur_llm::ask_prompt(&segments, &names, &question),
        &on_event,
    )
    .await
    {
        let _ = on_event.send(StreamEvent::Done { text });
    }
    Ok(())
}

/// Writes notes for a saved session from its transcript and the user's own notes,
/// then saves them to the session file and index. The user's notes are kept verbatim
/// under "My notes" so nothing they wrote is lost to the model.
#[tauri::command]
pub async fn generate_notes(
    app: AppHandle,
    session_id: String,
    user_notes: String,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    let (segments, names) = segments(&app, Some(&session_id))?;
    if segments.is_empty() {
        return Err("this session has no transcript to write notes from".into());
    }
    let prompt = murmur_llm::notes_prompt(&segments, &names, &user_notes);
    let Some(text) = run(&app, prompt, &on_event).await else {
        return Ok(());
    };
    let mut notes = strip_code_fence(&text).to_string();
    if !user_notes.trim().is_empty() {
        notes.push_str("\n\n## My notes\n\n");
        notes.push_str(user_notes.trim());
    }
    save_notes(app, session_id, notes.clone())?;
    let _ = on_event.send(StreamEvent::Done { text: notes });
    Ok(())
}

/// Some models wrap the whole answer in ```markdown fences.
fn strip_code_fence(text: &str) -> &str {
    let t = text.trim();
    t.strip_prefix("```markdown")
        .or_else(|| t.strip_prefix("```md"))
        .or_else(|| t.strip_prefix("```"))
        .and_then(|rest| rest.strip_suffix("```"))
        .map_or(t, str::trim)
}

/// Saves notes into the session file (between its notes markers) and the index.
#[tauri::command]
pub fn save_notes(app: AppHandle, session_id: String, notes: String) -> Result<(), String> {
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    let (record, _) = index
        .get_session(&session_id)
        .map_err(|e| e.to_string())?
        .ok_or("session not found")?;
    let format: ExportFormat = record.session.format;
    murmur_store::write_notes(std::path::Path::new(&record.file_path), format, &notes)
        .map_err(|e| format!("could not update {}: {e}", record.file_path))?;
    index.set_notes(&session_id, &notes).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_models(provider: Provider) -> Result<Vec<ModelInfo>, String> {
    let key = api_key(&provider);
    murmur_llm::list_models(&provider, key.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct ApiKeys {
    anthropic: bool,
    gemini: bool,
}

/// Which cloud providers have a key saved. The keys themselves never leave Rust.
#[tauri::command]
pub fn api_keys() -> ApiKeys {
    let saved = |name| passwords::get_generic_password(KEYCHAIN_SERVICE, name).is_ok();
    ApiKeys {
        anthropic: saved("anthropic"),
        gemini: saved("gemini"),
    }
}

/// Saves (or with `None`, removes) a cloud API key in the Keychain.
#[tauri::command]
pub fn set_api_key(name: String, key: Option<String>) -> Result<(), String> {
    if !matches!(name.as_str(), "anthropic" | "gemini") {
        return Err(format!("unknown provider {name}"));
    }
    match key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()) {
        Some(key) => passwords::set_generic_password(KEYCHAIN_SERVICE, &name, key.as_bytes())
            .map_err(|e| format!("could not save the key in the Keychain: {e}")),
        None => match passwords::delete_generic_password(KEYCHAIN_SERVICE, &name) {
            Ok(()) => Ok(()),
            // Removing a key that isn't there is fine.
            Err(e) if e.code() == KEY_NOT_FOUND => Ok(()),
            Err(e) => Err(format!("could not remove the key from the Keychain: {e}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::strip_code_fence;

    #[test]
    fn strips_markdown_fences() {
        assert_eq!(
            strip_code_fence("```markdown\n## Summary\n- a\n```"),
            "## Summary\n- a"
        );
        assert_eq!(strip_code_fence("## Summary\n- a"), "## Summary\n- a");
    }
}
