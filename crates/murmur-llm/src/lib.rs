//! LLM access for summaries and live "Ask" questions.
//!
//! Every provider receives the same message layout so the transcript stays an
//! append-only prefix: Anthropic prompt caching and Ollama's KV-cache reuse both
//! only pay for the new lines plus the question.

use murmur_core::{Channel, Segment};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Provider {
    /// Local inference. Default, nothing leaves the machine.
    Ollama {
        base_url: String,
        model: String,
        /// Ollama's default context is small and silently truncates; always set it.
        num_ctx: u32,
    },
    /// The API key itself lives in the macOS Keychain, never in config or the webview.
    Anthropic {
        model: String,
    },
    Gemini {
        model: String,
    },
}

impl Default for Provider {
    fn default() -> Self {
        Provider::Ollama {
            base_url: "http://127.0.0.1:11434".into(),
            model: "qwen3:14b".into(),
            num_ctx: 32_768,
        }
    }
}

impl Provider {
    pub fn is_local(&self) -> bool {
        matches!(self, Provider::Ollama { .. })
    }
}

pub const ASK_SYSTEM_PROMPT: &str = "You answer questions about a meeting that is still in progress. \
Use only the transcript provided. Cite the [mm:ss] timestamps you rely on. \
If the transcript doesn't contain the answer, say so plainly.";

/// Renders segments as `[mm:ss] Label: text` lines.
pub fn render_transcript(segments: &[Segment], label_for: impl Fn(&Segment) -> String) -> String {
    let mut out = String::new();
    for s in segments {
        let secs = s.start / 1000;
        out.push_str(&format!(
            "[{:02}:{:02}] {}: {}\n",
            secs / 60,
            secs % 60,
            label_for(s),
            s.text.trim()
        ));
    }
    out
}

/// Default label when diarization hasn't run yet.
pub fn channel_label(s: &Segment) -> String {
    match (&s.channel, &s.speaker) {
        (_, Some(id)) => id.0.clone(),
        (Channel::Mic, None) => "Me".into(),
        (Channel::System, None) => "Them".into(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// Builds the user turn for an Ask question. The transcript comes first and the
/// question last, so the cacheable prefix only ever grows.
pub fn ask_message(transcript: &str, question: &str) -> Message {
    Message {
        role: Role::User,
        content: format!("<transcript>\n{transcript}</transcript>\n\nQuestion: {question}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: u64, channel: Channel, text: &str) -> Segment {
        Segment {
            start,
            end: start + 1000,
            channel,
            speaker: None,
            text: text.into(),
        }
    }

    #[test]
    fn renders_timestamps_and_channel_labels() {
        let t = render_transcript(
            &[
                seg(5_000, Channel::Mic, " hi "),
                seg(65_000, Channel::System, "hello"),
            ],
            channel_label,
        );
        assert_eq!(t, "[00:05] Me: hi\n[01:05] Them: hello\n");
    }

    #[test]
    fn transcript_prefix_is_stable_as_it_grows() {
        let a = ask_message("[00:01] Me: a\n", "q1").content;
        let b = ask_message("[00:01] Me: a\n[00:02] Them: b\n", "q2").content;
        assert!(b.starts_with("<transcript>\n[00:01] Me: a\n"));
        assert!(a.starts_with("<transcript>\n[00:01] Me: a\n"));
    }
}
