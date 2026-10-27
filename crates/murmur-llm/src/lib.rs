//! LLM access for live "Ask" questions and post-meeting notes.
//!
//! Every provider receives the same layout: system prompt, then the transcript, then
//! the instruction. The transcript only grows during a meeting, so it stays a stable
//! prefix: Anthropic prompt caching and Ollama's KV cache both only pay for new lines
//! plus the question (ADR-004).

mod client;

pub use client::{ModelInfo, list_models, stream, warm_up};

use murmur_core::{Segment, SpeakerNames, speaker_label};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("no {0} API key saved. Add one in Settings.")]
    MissingKey(&'static str),
    #[error("{0}")]
    Http(String),
    #[error("{0}")]
    Api(String),
    #[error("the model declined to answer")]
    Refused,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    pub fn model(&self) -> &str {
        match self {
            Provider::Ollama { model, .. } | Provider::Anthropic { model } | Provider::Gemini { model } => {
                model
            }
        }
    }

    /// Keychain account holding this provider's API key; `None` for local providers.
    pub fn key_name(&self) -> Option<&'static str> {
        match self {
            Provider::Ollama { .. } => None,
            Provider::Anthropic { .. } => Some("anthropic"),
            Provider::Gemini { .. } => Some("gemini"),
        }
    }

    /// Short label for the UI, e.g. "Local · qwen3:14b".
    pub fn label(&self) -> String {
        let place = if self.is_local() { "Local" } else { "Cloud" };
        format!("{place} · {}", self.model())
    }
}

/// One request: the transcript is the cacheable prefix, the instruction comes last.
#[derive(Debug, Clone)]
pub struct Prompt {
    pub system: String,
    pub transcript: String,
    pub instruction: String,
    pub max_tokens: u32,
}

impl Prompt {
    pub fn transcript_block(&self) -> String {
        format!("<transcript>\n{}</transcript>", self.transcript)
    }
}

const SPEAKERS: &str = "Lines are `[mm:ss] Speaker: text`. \"Me\" is the user. Other speakers are people on \
the call: by name, as \"Speaker 2\" when not yet named, or \"Them\" when unknown (possibly several people). \
When the transcript makes clear who a numbered speaker is (someone calls on a person by name and that \
speaker answers, or they introduce themselves), use the name with the label, like \"Priya (Speaker 5)\". \
Speaker labels can be wrong for a line or two, so go by what most of their lines show.";

/// A question asked during or after the meeting ("/btw …").
pub fn ask_prompt(segments: &[Segment], names: &SpeakerNames, question: &str) -> Prompt {
    Prompt {
        system: format!(
            "You answer questions about a meeting, often while it is still in progress. {SPEAKERS} \
Use only the transcript. Cite the timestamps you rely on, like [12:04]. Be brief: one or two \
sentences or a few bullets. Answer in the language of the question. If the transcript doesn't \
cover it, say so plainly."
        ),
        transcript: render_transcript(segments, names),
        instruction: format!("Question: {}", question.trim()),
        max_tokens: 2_000,
    }
}

/// Post-meeting notes. `user_notes` are what the user typed during the meeting; every
/// point in them must be covered.
pub fn notes_prompt(segments: &[Segment], names: &SpeakerNames, user_notes: &str) -> Prompt {
    let instruction = if user_notes.trim().is_empty() {
        "Write the notes for this meeting.".to_string()
    } else {
        format!(
            "Write the notes for this meeting. The user jotted these during the meeting; make sure \
every point in them is covered and expanded with what was said:\n<user_notes>\n{}\n</user_notes>",
            user_notes.trim()
        )
    };
    Prompt {
        system: format!(
            "You write meeting notes from a transcript. {SPEAKERS}\n\n\
Output Markdown with these sections. Leave a section out entirely when there is nothing for it; \
never write \"none\" or similar:\n\
## Summary\n3 to 6 bullets with the main points.\n\
## Decisions\nWhat was agreed.\n\
## Action items\n`- [ ] Who: task (due date if one was mentioned)`.\n\
## Open questions\n\n\
End each bullet with the timestamp it comes from, like [12:04]. Be specific (names, numbers, \
dates). Never invent anything that isn't in the transcript. Write in the language most of the \
meeting was held in. Output only the notes, with no preamble."
        ),
        transcript: render_transcript(segments, names),
        instruction,
        max_tokens: 4_000,
    }
}

/// Renders segments as `[mm:ss] Speaker: text` lines.
pub fn render_transcript(segments: &[Segment], names: &SpeakerNames) -> String {
    let mut out = String::new();
    for s in segments {
        let secs = s.start / 1000;
        out.push_str(&format!(
            "[{:02}:{:02}] {}: {}\n",
            secs / 60,
            secs % 60,
            speaker_label(s.channel, s.speaker.as_ref(), names),
            s.text.trim()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use murmur_core::{Channel, SpeakerId};

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
    fn renders_timestamps_and_speaker_names() {
        let mut ana = seg(70_000, Channel::System, "hola");
        ana.speaker = Some(SpeakerId("speaker_1".into()));
        let names = SpeakerNames::from([("speaker_1".to_string(), "Ana".to_string())]);
        let t = render_transcript(
            &[
                seg(5_000, Channel::Mic, " hi "),
                seg(65_000, Channel::System, "hello"),
                ana,
            ],
            &names,
        );
        assert_eq!(t, "[00:05] Me: hi\n[01:05] Them: hello\n[01:10] Ana: hola\n");
    }

    #[test]
    fn transcript_prefix_is_stable_as_it_grows() {
        let first = [seg(1_000, Channel::Mic, "a")];
        let later = [seg(1_000, Channel::Mic, "a"), seg(2_000, Channel::System, "b")];
        let a = ask_prompt(&first, &SpeakerNames::new(), "q1");
        let b = ask_prompt(&later, &SpeakerNames::new(), "q2");
        assert_eq!(a.system, b.system);
        assert!(
            b.transcript_block()
                .starts_with(a.transcript_block().trim_end_matches("</transcript>"))
        );
    }

    #[test]
    fn notes_prompt_includes_user_notes_only_when_present() {
        let segments = [seg(1_000, Channel::Mic, "a")];
        assert!(
            !notes_prompt(&segments, &SpeakerNames::new(), "  ")
                .instruction
                .contains("user_notes")
        );
        assert!(
            notes_prompt(&segments, &SpeakerNames::new(), "ship friday")
                .instruction
                .contains("ship friday")
        );
    }
}
