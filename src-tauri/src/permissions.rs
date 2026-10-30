//! Whether macOS lets Murmur record the mic and system audio, and a way to the right
//! System Settings pane when it doesn't.

use murmur_audio::Permissions;
use serde::Deserialize;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::recording;

#[tauri::command]
pub async fn permissions() -> Result<Permissions, String> {
    let helper = recording::helper_path()?;
    tauri::async_runtime::spawn_blocking(move || Permissions::check(&helper))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Mic,
    System,
}

#[tauri::command]
pub fn open_privacy_settings(app: AppHandle, source: Source) -> Result<(), String> {
    // System audio is under "Screen & System Audio Recording", in its own section.
    let pane = match source {
        Source::Mic => "Privacy_Microphone",
        Source::System => "Privacy_ScreenCapture",
    };
    app.opener()
        .open_url(
            format!("x-apple.systempreferences:com.apple.preference.security?{pane}"),
            None::<&str>,
        )
        .map_err(|e| e.to_string())
}
