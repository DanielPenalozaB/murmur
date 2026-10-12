mod recording;

use murmur_llm::Provider;
use serde::Serialize;

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
        .invoke_handler(tauri::generate_handler![
            app_info,
            recording::start_recording,
            recording::set_recording_paused,
            recording::stop_recording,
            recording::recording_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
