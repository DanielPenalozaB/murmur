mod library;
mod recording;
mod settings;

use std::sync::Mutex;

use murmur_llm::Provider;
use serde::Serialize;
use tauri::Manager;

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    llm: String,
    local: bool,
}

#[tauri::command]
fn app_info() -> AppInfo {
    let provider = Provider::default();
    let llm = match &provider {
        Provider::Ollama { model, .. } => format!("Local · {model}"),
        Provider::Anthropic { model } | Provider::Gemini { model } => format!("Cloud · {model}"),
    };
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        local: provider.is_local(),
        llm,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(recording::RecordingState::default())
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
            recording::start_recording,
            recording::set_recording_paused,
            recording::stop_recording,
            recording::recording_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
