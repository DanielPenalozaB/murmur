//! Renaming, moving and deleting sessions and folders from the app, and how much
//! space the recordings take.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::library::LibraryState;
use crate::retranscribe::{self, session_dir};
use crate::settings::SettingsState;

/// The recordings a session keeps for re-transcribing; the rest of its folder is small.
const AUDIO_FILES: [&str; 2] = ["mic.wav", "system.wav"];

fn vault(app: &AppHandle) -> PathBuf {
    app.state::<SettingsState>().0.lock().unwrap().vault_path.clone()
}

/// Moving files under a re-transcription would lose its result, or its audio.
fn ensure_idle(app: &AppHandle) -> Result<(), String> {
    if retranscribe::is_running(app) {
        return Err("wait for the re-transcription to finish".into());
    }
    Ok(())
}

#[tauri::command]
pub fn rename_session(app: AppHandle, id: String, title: String) -> Result<(), String> {
    ensure_idle(&app)?;
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    let (record, _) = index
        .get_session(&id)
        .map_err(|e| e.to_string())?
        .ok_or("session not found")?;
    murmur_store::relocate_session(&index, &vault(&app), &id, &title, &record.session.folder)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn move_session(app: AppHandle, id: String, folder: String) -> Result<(), String> {
    ensure_idle(&app)?;
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    let (record, _) = index
        .get_session(&id)
        .map_err(|e| e.to_string())?
        .ok_or("session not found")?;
    murmur_store::relocate_session(&index, &vault(&app), &id, &record.session.title, &folder)
        .map_err(|e| e.to_string())
}

/// Deletes the session file, its recordings and its place in the library.
#[tauri::command]
pub fn delete_session(app: AppHandle, id: String) -> Result<(), String> {
    ensure_idle(&app)?;
    let dir = session_dir(&app, &id)?;
    {
        let library = app.state::<LibraryState>();
        let index = library.0.lock().unwrap();
        murmur_store::delete_session(&index, &id).map_err(|e| e.to_string())?;
    }
    match std::fs::remove_dir_all(&dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}

/// Frees the space the recordings take, keeping the transcript and notes. The session
/// can't be re-transcribed afterwards.
#[tauri::command]
pub fn delete_session_audio(app: AppHandle, id: String) -> Result<(), String> {
    ensure_idle(&app)?;
    let dir = session_dir(&app, &id)?;
    for name in AUDIO_FILES {
        match std::fs::remove_file(dir.join(name)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            _ => {}
        }
    }
    Ok(())
}

/// Renames a folder (`to` in the same parent) or moves it elsewhere in the vault.
#[tauri::command]
pub fn move_folder(app: AppHandle, from: String, to: String) -> Result<(), String> {
    ensure_idle(&app)?;
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    murmur_store::move_folder(&index, &vault(&app), &from, to.trim()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_folder(app: AppHandle, folder: String) -> Result<(), String> {
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    murmur_store::delete_folder(&index, &vault(&app), &folder).map_err(|e| e.to_string())
}

/// Catches up with sessions moved or renamed in Finder; returns how many were found.
#[tauri::command]
pub fn sync_vault(app: AppHandle) -> Result<usize, String> {
    if retranscribe::is_running(&app) {
        return Ok(0);
    }
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    murmur_store::sync_vault(&index, &vault(&app)).map_err(|e| e.to_string())
}

/// Bytes of a session's recordings; 0 once they're deleted.
pub fn audio_bytes(app: &AppHandle, id: &str) -> u64 {
    session_dir(app, id).map_or(0, |dir| recordings_size(&dir))
}

fn recordings_size(dir: &Path) -> u64 {
    AUDIO_FILES
        .iter()
        .filter_map(|name| std::fs::metadata(dir.join(name)).ok())
        .map(|m| m.len())
        .sum()
}

#[derive(Serialize)]
pub struct StorageUsage {
    audio_bytes: u64,
    /// Sessions that still have their recordings.
    sessions_with_audio: u32,
}

#[tauri::command]
pub fn storage_usage(app: AppHandle) -> Result<StorageUsage, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("sessions");
    let mut usage = StorageUsage {
        audio_bytes: 0,
        sessions_with_audio: 0,
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Ok(usage);
    };
    for entry in entries.flatten() {
        let size = recordings_size(&entry.path());
        if size > 0 {
            usage.audio_bytes += size;
            usage.sessions_with_audio += 1;
        }
    }
    Ok(usage)
}
