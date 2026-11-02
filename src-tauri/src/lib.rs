mod actions;
mod assistant;
mod library;
mod logging;
mod manage;
mod meeting;
mod models;
mod moments;
mod permissions;
mod playback;
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
        .plugin(tauri_plugin_notification::init())
        .plugin(moments::plugin())
        .register_asynchronous_uri_scheme_protocol(playback::SCHEME, playback::handle)
        .manage(recording::RecordingState::default())
        .manage(speakers::SpeakersState::default())
        .manage(retranscribe::RetranscribeState::default())
        .manage(models::DownloadState::default())
        .manage(meeting::MeetingState::default())
        .manage(moments::MomentsState::default())
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
            manage::restore_session_file,
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
            actions::task_notes,
            actions::set_action_done,
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
            moments::moment_card,
            moments::dismiss_moment_card,
            moments::open_moment,
            moments::submit_quick_note,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            // Clicking the Dock icon brings back a closed window.
            tauri::RunEvent::Reopen { .. } => tray::show_main(app),
            // Quit, the menu bar's Quit and the restart after an update all end here.
            tauri::RunEvent::Exit => quit(app),
            _ => {}
        });
}

/// whisper.cpp's Metal backend aborts the process in its own cleanup at exit when a
/// model is still loaded, so free the models first. One still in use (quitting while
/// recording or re-transcribing) can't be freed under the thread using it; then end the
/// process without running that cleanup at all.
fn quit(app: &tauri::AppHandle) {
    if recording::release_models(app) {
        return;
    }
    tracing::warn!("quitting while a Whisper model is in use; skipping native cleanup");
    unsafe extern "C" {
        fn _exit(status: i32) -> !;
    }
    // SAFETY: ends the process at once, as `exit` would, minus the C++ destructors.
    unsafe { _exit(0) }
}
