mod assistant;
mod library;
mod manage;
mod models;
mod recording;
mod retranscribe;
mod settings;
mod speakers;

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
        .plugin(tauri_plugin_opener::init())
        .manage(recording::RecordingState::default())
        .manage(speakers::SpeakersState::default())
        .manage(retranscribe::RetranscribeState::default())
        .manage(models::DownloadState::default())
        .setup(|app| {
            let handle = app.handle();
            app.manage(settings::SettingsState(Mutex::new(settings::load(handle))));
            app.manage(library::LibraryState(Mutex::new(library::open_index(handle)?)));
            recording::preload_model(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            settings::get_settings,
            settings::update_settings,
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
            speakers::sessions_with_speaker,
            models::missing_models,
            models::download_models,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
