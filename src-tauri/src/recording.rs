use std::path::PathBuf;
use std::sync::Mutex;

use murmur_audio::{Recorder, RecorderStatus, Recording};
use tauri::{AppHandle, Manager, State};

#[derive(Default)]
pub struct RecordingState(Mutex<Option<Recorder>>);

/// The `audiocap` sidecar sits next to the app binary (Tauri copies `externalBin` there).
fn helper_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(exe.with_file_name("audiocap"))
}

#[tauri::command]
pub fn start_recording(app: AppHandle, state: State<RecordingState>) -> Result<(), String> {
    let mut slot = state.0.lock().unwrap();
    if slot.is_some() {
        return Err("a recording is already running".into());
    }
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("sessions")
        .join(stamp);
    let recorder = Recorder::start(&helper_path()?, &dir).map_err(|e| e.to_string())?;
    *slot = Some(recorder);
    Ok(())
}

#[tauri::command]
pub fn set_recording_paused(paused: bool, state: State<RecordingState>) -> Result<(), String> {
    let slot = state.0.lock().unwrap();
    let recorder = slot.as_ref().ok_or("not recording")?;
    recorder.set_paused(paused);
    Ok(())
}

#[tauri::command]
pub fn stop_recording(state: State<RecordingState>) -> Result<Recording, String> {
    let recorder = state.0.lock().unwrap().take().ok_or("not recording")?;
    recorder.stop().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn recording_status(state: State<RecordingState>) -> Option<RecorderStatus> {
    state.0.lock().unwrap().as_ref().map(Recorder::status)
}
