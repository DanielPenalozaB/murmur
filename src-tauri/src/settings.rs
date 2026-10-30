use std::path::PathBuf;
use std::sync::Mutex;

use murmur_core::ExportFormat;
use murmur_llm::Provider;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::library::LibraryState;
use crate::{recording, retranscribe};

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
    /// Names and terms hinted to Whisper so it spells them right (people, products,
    /// ticket prefixes). Remembered speakers' names are added automatically.
    pub vocabulary: Vec<String>,
    /// Offer to record when a calling app starts using the microphone.
    pub detect_meetings: bool,
    /// Calling apps (as named in the offer, e.g. "Slack") never offered for.
    pub ignored_meeting_apps: Vec<String>,
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
            vocabulary: Vec::new(),
            detect_meetings: true,
            ignored_meeting_apps: Vec::new(),
        }
    }
}

impl Settings {
    /// Whether a call in `app` (as named in the offer) gets an offer to record.
    pub fn offers_for(&self, app: &str) -> bool {
        self.detect_meetings && !self.ignored_meeting_apps.iter().any(|a| a == app)
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

/// Takes the sessions along to a new vault folder, so the library and the sidebar's
/// notebooks keep agreeing.
fn move_vault(app: &AppHandle, from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    // Both write into the vault when they finish.
    if recording::is_recording(app) || retranscribe::is_running(app) {
        return Err("change the vault folder once the recording or re-transcription finishes".into());
    }
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    let moved = murmur_store::move_vault(&index, from, to).map_err(|e| e.to_string())?;
    let found = murmur_store::sync_vault(&index, to).map_err(|e| e.to_string())?;
    tracing::info!(
        "vault moved to {}: {moved} sessions moved, {found} found there",
        to.display()
    );
    Ok(())
}

/// Finder's folder picker, starting at the current vault. `None` when cancelled.
#[tauri::command]
pub async fn pick_vault_folder(app: AppHandle) -> Option<PathBuf> {
    let current = app.state::<SettingsState>().0.lock().unwrap().vault_path.clone();
    let start = if current.is_dir() {
        current
    } else {
        dirs_home(&app)
    };
    app.dialog()
        .file()
        .set_title("Choose the vault folder")
        .set_directory(start)
        .blocking_pick_folder()
        .and_then(|path| path.into_path().ok())
}

fn dirs_home(app: &AppHandle) -> PathBuf {
    app.path().home_dir().unwrap_or_else(|_| PathBuf::from("/"))
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
    let old_vault = state.0.lock().unwrap().vault_path.clone();
    if settings.vault_path != old_vault {
        move_vault(&app, &old_vault, &settings.vault_path)?;
    }
    save(&app, &settings)?;
    *state.0.lock().unwrap() = settings;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignored_apps_and_the_switch_stop_offers() {
        let mut settings = Settings {
            ignored_meeting_apps: vec!["Slack".into()],
            ..Settings::default()
        };
        assert!(settings.offers_for("Microsoft Teams"));
        assert!(!settings.offers_for("Slack"));
        settings.detect_meetings = false;
        assert!(!settings.offers_for("Microsoft Teams"));
    }

    #[test]
    fn older_settings_files_load_without_the_ignore_list() {
        let settings: Settings = serde_json::from_str(r#"{"detect_meetings": true}"#).unwrap();
        assert!(settings.ignored_meeting_apps.is_empty());
    }
}
