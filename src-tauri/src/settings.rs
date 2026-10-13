use std::path::PathBuf;
use std::sync::Mutex;

use murmur_core::ExportFormat;
use murmur_llm::Provider;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Root folder for transcript files. Defaults to ~/Documents/Murmur.
    pub vault_path: PathBuf,
    pub default_format: ExportFormat,
    /// Preselected in the New session dialog.
    pub last_folder: String,
    /// ISO codes of the languages spoken in meetings. Empty lets Whisper detect each
    /// chunk, one forces it, two or more follow switches between them.
    pub languages: Vec<String>,
    /// Model for Ask and notes. Local Ollama unless the user opts into a cloud API.
    pub llm: Provider,
    /// Write notes automatically when a recording finishes.
    pub auto_notes: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        Settings {
            vault_path: home.join("Documents").join("Murmur"),
            default_format: ExportFormat::Markdown,
            last_folder: String::new(),
            languages: Vec::new(),
            llm: Provider::default(),
            auto_notes: true,
        }
    }
}

pub struct SettingsState(pub Mutex<Settings>);

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json"))
}

/// Reads settings.json, falling back to defaults when it's missing or unreadable.
pub fn load(app: &AppHandle) -> Settings {
    settings_path(app)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app)?;
    std::fs::create_dir_all(path.parent().expect("config path has a parent")).map_err(|e| e.to_string())?;
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(state: State<SettingsState>) -> Settings {
    state.0.lock().unwrap().clone()
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    settings: Settings,
    state: State<SettingsState>,
) -> Result<(), String> {
    if !settings.vault_path.is_absolute() {
        return Err("the vault folder must be an absolute path".into());
    }
    murmur_store::validate_folder(&settings.last_folder).map_err(|e| e.to_string())?;
    save(&app, &settings)?;
    *state.0.lock().unwrap() = settings;
    Ok(())
}
