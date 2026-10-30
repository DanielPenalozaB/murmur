//! Persistence. Markdown/TXT files in the vault are the user-facing source of truth
//! (ADR-003); SQLite is an index of sessions and segments for listing and search.

mod export;
mod index;
mod notes;
mod vault;

pub use export::{
    Turn, file_extension, file_name, group_turns, render_session_file, render_transcript, replace_notes,
    replace_transcript, write_notes, write_session_file, write_transcript,
};
pub use index::{
    Index, SearchHit, SessionCard, SessionRecord, SessionSpeaker, SpeakerInfo, UnnamedSpeaker, VoiceEntry,
};
pub use notes::{count_actions, excerpt};
pub use vault::{FolderNode, create_folder, list_folders, validate_folder};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("invalid folder name: {0}")]
    InvalidFolder(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error("{0} not found")]
    NotFound(String),
    #[error("the session file doesn't have the layout Murmur writes")]
    Unrecognized,
}
