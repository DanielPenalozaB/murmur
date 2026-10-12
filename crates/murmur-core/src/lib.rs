//! Domain types shared across Murmur crates. No I/O lives here.

use serde::{Deserialize, Serialize};

/// Milliseconds since the session started recording (pauses excluded).
pub type Millis = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// The local microphone: always the user.
    Mic,
    /// System audio (Teams / Meet / Slack): everyone else.
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpeakerId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Speaker {
    pub id: SpeakerId,
    /// Display name; defaults to "Me" / "Speaker 1" until the user renames it.
    pub label: String,
}

/// One finalized chunk of transcribed speech.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start: Millis,
    pub end: Millis,
    pub channel: Channel,
    /// `None` until diarization assigns a speaker (System channel only).
    pub speaker: Option<SpeakerId>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Recording,
    Paused,
    Processing,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    #[default]
    Markdown,
    Txt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    /// Folder path relative to the vault root, e.g. `Work/1-1s`.
    pub folder: String,
    /// RFC 3339 timestamp.
    pub started_at: String,
    pub state: SessionState,
    pub speakers: Vec<Speaker>,
}
