mod assistant;
mod library;
mod logging;
mod manage;
mod meeting;
mod models;
mod permissions;
mod recording;
mod retranscribe;
mod settings;
mod speakers;
mod tray;

use std::sync::Mutex;

use serde::Serialize;
use tauri::{Manager, State};

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    llm: String,
    local: bool,
}

#[tauri::command]
fn app_info(settings: State<settings::SettingsState>) -> AppInfo {
    let provider = settings.0.lock().unwrap().llm.clone();
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        local: provider.is_local(),
        llm: provider.label(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(recording::RecordingState::default())
        .manage(speakers::SpeakersState::default())
        .manage(retranscribe::RetranscribeState::default())
        .manage(models::DownloadState::default())
        .manage(meeting::MeetingState::default())
        .setup(|app| {
            let handle = app.handle();
            if let Ok(dir) = app.path().app_log_dir() {
                logging::init(&dir);
            }
            app.manage(settings::SettingsState(Mutex::new(settings::load(handle))));
            app.manage(library::LibraryState(Mutex::new(library::open_index(handle)?)));
            recording::preload_model(handle);
            meeting::watch(handle);
            tray::install(handle)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                // Back in Murmur, the in-app banner takes over from the floating card.
                tauri::WindowEvent::Focused(true) => meeting::close_popup(window.app_handle()),
                // Murmur keeps running in the menu bar; the webview stays alive so
                // recordings started from there still reach the transcript and notes.
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            settings::get_settings,
            settings::update_settings,
            settings::pick_vault_folder,
            library::list_folders,
            library::create_folder,
            library::list_sessions,
            library::get_session,
            library::search,
            library::open_session_item,
            manage::rename_session,
            manage::move_session,
            manage::delete_session,
            manage::delete_session_audio,
            manage::move_folder,
            manage::delete_folder,
            manage::storage_usage,
            manage::sync_vault,
            permissions::permissions,
            permissions::open_privacy_settings,
            recording::start_recording,
            recording::set_recording_paused,
            recording::set_mic_muted,
            recording::stop_recording,
            recording::recording_status,
            retranscribe::retranscribe_session,
            retranscribe::session_languages,
            assistant::ask,
            assistant::generate_notes,
            assistant::save_notes,
            assistant::list_models,
            assistant::api_keys,
            assistant::set_api_key,
            speakers::rename_speaker,
            speakers::list_voices,
            speakers::forget_voice,
            speakers::decline_suggestion,
            speakers::unname_speaker,
            speakers::speaker_review,
            speakers::dismiss_review,
            speakers::sessions_with_speaker,
            models::missing_models,
            models::download_models,
            meeting::detected_meeting,
            meeting::dismiss_meeting,
            meeting::ignore_meeting_app,
            meeting::collapse_meeting_popup,
            meeting::record_meeting,
            meeting::stop_meeting,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Clicking the Dock icon brings back a closed window.
            if let tauri::RunEvent::Reopen { .. } = event {
                tray::show_main(app);
            }
        });
}
