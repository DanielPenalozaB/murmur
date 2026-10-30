//! Persistence. Markdown/TXT files in the vault are the user-facing source of truth
//! (ADR-003); SQLite is an index of sessions and segments for listing and search.

mod export;
mod index;
mod manage;
mod notes;
mod vault;

pub use export::{
    Turn, file_extension, file_name, group_turns, render_session_file, render_transcript, replace_notes,
    replace_transcript, write_notes, write_session_file, write_transcript,
};
pub use index::{Index, SearchHit, SessionCard, SessionRecord, SessionSpeaker, SpeakerInfo, VoiceEntry};
pub use manage::{delete_folder, delete_session, move_folder, relocate_session, replace_header, sync_vault};
pub use notes::{count_actions, excerpt};
pub use vault::{FolderNode, create_folder, list_folders, validate_folder};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("invalid folder name: {0}")]
    InvalidFolder(String),
    #[error("a folder named {0} already exists")]
    FolderExists(String),
    #[error("{0} still has notes or other files in it")]
    FolderNotEmpty(String),
    #[error("enter a title of up to 200 characters, on one line")]
    InvalidTitle,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
    #[error("{0} not found")]
    NotFound(String),
    #[error("the session file doesn't have the layout Murmur writes")]
    Unrecognized,
}
