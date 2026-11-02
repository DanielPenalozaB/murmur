//! Action items across meetings. The notes in each session file are the source of
//! truth: the UI parses the task lines, and ticking one rewrites that line.

use murmur_store::SessionRecord;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::assistant::write_notes;
use crate::library::LibraryState;

#[derive(Serialize)]
pub struct TaskNotes {
    #[serde(flatten)]
    record: SessionRecord,
    notes: String,
    /// Whether the recording is still there to play the moment a task was agreed.
    has_audio: bool,
}

/// Meetings whose notes have action items, newest first.
#[tauri::command]
pub fn task_notes(app: AppHandle) -> Result<Vec<TaskNotes>, String> {
    let library = app.state::<LibraryState>();
    let found = library
        .0
        .lock()
        .unwrap()
        .sessions_with_tasks()
        .map_err(|e| e.to_string())?;
    Ok(found
        .into_iter()
        .map(|(record, notes)| TaskNotes {
            has_audio: crate::manage::audio_bytes(&app, &record.session.id) > 0,
            record,
            notes,
        })
        .collect())
}

/// Ticks or unticks the task on `line` of a session's notes, in its file too.
#[tauri::command]
pub fn set_action_done(app: AppHandle, session_id: String, line: usize, done: bool) -> Result<(), String> {
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    let notes = index.notes(&session_id).map_err(|e| e.to_string())?;
    let updated = murmur_store::set_task(&notes, line, done)
        .ok_or("the notes changed since this list was loaded; try again")?;
    write_notes(&index, &session_id, &updated)
}
