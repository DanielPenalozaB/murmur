use std::collections::HashMap;
use std::sync::Mutex;

use murmur_core::Segment;
use murmur_store::{FolderNode, Index, SearchHit, SessionCard, SessionRecord, SpeakerInfo};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::settings::SettingsState;

pub struct LibraryState(pub Mutex<Index>);

#[derive(Serialize)]
pub struct Folders {
    tree: Vec<FolderNode>,
    /// Session count per folder path ("" is the vault root).
    counts: HashMap<String, u32>,
}

#[derive(Serialize)]
pub struct SessionDetail {
    #[serde(flatten)]
    record: SessionRecord,
    segments: Vec<Segment>,
    notes: String,
    speakers: Vec<SpeakerInfo>,
}

pub fn open_index(app: &AppHandle) -> Result<Index, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Index::open(&dir.join("murmur.db")).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_folders(settings: State<SettingsState>, library: State<LibraryState>) -> Result<Folders, String> {
    let vault = settings.0.lock().unwrap().vault_path.clone();
    let tree = murmur_store::list_folders(&vault).map_err(|e| e.to_string())?;
    let counts = library
        .0
        .lock()
        .unwrap()
        .folder_counts()
        .map_err(|e| e.to_string())?;
    Ok(Folders {
        tree,
        counts: counts.into_iter().collect(),
    })
}

#[tauri::command]
pub fn create_folder(path: String, settings: State<SettingsState>) -> Result<(), String> {
    let vault = settings.0.lock().unwrap().vault_path.clone();
    murmur_store::create_folder(&vault, path.trim()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_sessions(
    folder: Option<String>,
    library: State<LibraryState>,
) -> Result<Vec<SessionCard>, String> {
    library
        .0
        .lock()
        .unwrap()
        .session_cards(folder.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_session(id: String, library: State<LibraryState>) -> Result<SessionDetail, String> {
    let index = library.0.lock().unwrap();
    let found = index.get_session(&id).map_err(|e| e.to_string())?;
    let (record, segments) = found.ok_or("session not found")?;
    let notes = index.notes(&id).map_err(|e| e.to_string())?;
    let mut speakers = index.speakers(&id).map_err(|e| e.to_string())?;
    crate::speakers::suggest(&index, &id, &mut speakers).map_err(|e| e.to_string())?;
    Ok(SessionDetail {
        record,
        segments,
        notes,
        speakers,
    })
}

#[tauri::command]
pub fn search(query: String, library: State<LibraryState>) -> Result<Vec<SearchHit>, String> {
    library
        .0
        .lock()
        .unwrap()
        .search(&query, 50)
        .map_err(|e| e.to_string())
}

/// What to show for a session. Paths come from the index, never from the webview.
#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTarget {
    /// Open the transcript file in its default app.
    OpenFile,
    RevealFile,
    RevealAudio,
}

#[tauri::command]
pub fn open_session_item(
    app: AppHandle,
    id: String,
    target: SessionTarget,
    library: State<LibraryState>,
) -> Result<(), String> {
    let found = library
        .0
        .lock()
        .unwrap()
        .get_session(&id)
        .map_err(|e| e.to_string())?;
    let (record, _) = found.ok_or("session not found")?;
    let opener = app.opener();
    match target {
        SessionTarget::OpenFile => opener.open_path(&record.file_path, None::<&str>),
        SessionTarget::RevealFile => opener.reveal_item_in_dir(&record.file_path),
        SessionTarget::RevealAudio => {
            let audio = app
                .path()
                .app_data_dir()
                .map_err(|e| e.to_string())?
                .join("sessions")
                .join(&id)
                .join("mic.wav");
            opener.reveal_item_in_dir(audio)
        }
    }
    .map_err(|e| e.to_string())
}
