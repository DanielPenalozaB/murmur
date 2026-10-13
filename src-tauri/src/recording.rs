use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use murmur_audio::{Recorder, RecorderStatus, Recording, SpeechChunk};
use murmur_core::{ExportFormat, Segment, Session};
use murmur_store::SessionRecord;
use murmur_stt::{MeetingTranscriber, Transcriber, WhisperModel};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::library::LibraryState;
use crate::settings::{self, SettingsState};

const WHISPER_MODEL: &str = "ggml-large-v3-turbo-q5_0.bin";
/// Small and fast; only identifies languages in meetings that mix them.
const LANGUAGE_MODEL: &str = "ggml-base.bin";

/// Each language is one more candidate for every check, so keep meetings to a few.
const MAX_LANGUAGES: usize = 4;

#[derive(Default)]
pub struct RecordingState {
    active: Mutex<Option<ActiveSession>>,
    starting: AtomicBool,
    /// Loaded at launch and kept for the app's lifetime.
    model: Mutex<Option<Arc<WhisperModel>>>,
    /// Loaded the first time a meeting has two or more languages.
    language_model: Mutex<Option<Arc<WhisperModel>>>,
}

struct ActiveSession {
    recorder: Recorder,
    /// Sends the final duration to the transcription worker once recording stops.
    duration_tx: Sender<u64>,
}

/// Streamed to the UI over the channel passed to `start_recording`.
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TranscriptEvent {
    Segment {
        segment: Segment,
    },
    Error {
        message: String,
    },
    /// Everything is transcribed and the session file is written and indexed.
    Finished {
        session: Option<SessionRecord>,
    },
}

/// The `audiocap` sidecar sits next to the app binary (Tauri copies `externalBin` there).
fn helper_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(exe.with_file_name("audiocap"))
}

fn load_model(
    app: &AppHandle,
    slot: &Mutex<Option<Arc<WhisperModel>>>,
    file: &str,
) -> Result<Arc<WhisperModel>, String> {
    let mut slot = slot.lock().unwrap();
    if let Some(model) = slot.as_ref() {
        return Ok(model.clone());
    }
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models")
        .join(file);
    let model = WhisperModel::load(&path)
        .map_err(|e| format!("{e}. Run scripts/download-models.sh to download it."))?;
    *slot = Some(model.clone());
    Ok(model)
}

fn transcriber(app: &AppHandle, languages: &[String]) -> Result<MeetingTranscriber, String> {
    let state = app.state::<RecordingState>();
    let model = load_model(app, &state.model, WHISPER_MODEL)?;
    let detector = match languages.len() {
        0 | 1 => None,
        _ => Some(load_model(app, &state.language_model, LANGUAGE_MODEL)?),
    };
    MeetingTranscriber::new(&model, detector.as_ref(), languages).map_err(|e| e.to_string())
}

/// Loads the models in the background at launch so the first Start is instant.
pub fn preload_model(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        let languages = app.state::<SettingsState>().0.lock().unwrap().languages.clone();
        if let Err(e) = transcriber(&app, &languages) {
            tracing::warn!("model preload failed: {e}");
        }
    });
}

/// Async so the slow parts (model load, helper startup) run off the main thread and
/// the window stays responsive.
#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    title: String,
    folder: String,
    format: ExportFormat,
    languages: Vec<String>,
    on_event: Channel<TranscriptEvent>,
) -> Result<(), String> {
    murmur_store::validate_folder(&folder).map_err(|e| e.to_string())?;
    if languages.len() > MAX_LANGUAGES {
        return Err(format!("choose at most {MAX_LANGUAGES} languages"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<RecordingState>();
        // Guards against a second click while this start is still in progress. The
        // session mutex is only held briefly, so status polling never blocks.
        if state.active.lock().unwrap().is_some()
            || state
                .starting
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return Err("a recording is already running".into());
        }
        let result = (|| {
            let transcriber = transcriber(&app, &languages)?;
            let now = chrono::Local::now();
            let session = Session {
                id: now.format("%Y-%m-%d_%H-%M-%S").to_string(),
                title: if title.trim().is_empty() {
                    "Meeting".into()
                } else {
                    title.trim().to_string()
                },
                folder,
                started_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
                duration_ms: 0,
                format,
            };
            remember_choices(&app, &session, &languages);

            let dir = app
                .path()
                .app_data_dir()
                .map_err(|e| e.to_string())?
                .join("sessions")
                .join(&session.id);
            let (chunks_tx, chunks_rx) = mpsc::channel();
            let recorder =
                Recorder::start(&helper_path()?, &dir, Some(chunks_tx)).map_err(|e| e.to_string())?;
            let (duration_tx, duration_rx) = mpsc::channel();
            spawn_transcription(
                app.clone(),
                Worker {
                    session,
                    transcriber,
                    chunks: chunks_rx,
                    duration: duration_rx,
                    dir,
                    events: on_event,
                },
            );
            *state.active.lock().unwrap() = Some(ActiveSession {
                recorder,
                duration_tx,
            });
            Ok(())
        })();
        state.starting.store(false, Ordering::Release);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Preselects this folder, format and languages in the next New session dialog.
fn remember_choices(app: &AppHandle, session: &Session, languages: &[String]) {
    let state = app.state::<SettingsState>();
    let mut s = state.0.lock().unwrap();
    s.last_folder = session.folder.clone();
    s.default_format = session.format;
    s.languages = languages.to_vec();
    if let Err(e) = settings::save(app, &s) {
        tracing::warn!("save settings: {e}");
    }
}

struct Worker<T> {
    session: Session,
    transcriber: T,
    chunks: Receiver<SpeechChunk>,
    duration: Receiver<u64>,
    /// Session audio folder; `transcript.jsonl` is appended here as segments arrive.
    dir: PathBuf,
    events: Channel<TranscriptEvent>,
}

/// Transcribes chunks in arrival order until the recorder drops its senders on stop,
/// then writes the session file into the vault, indexes it and reports `Finished`.
fn spawn_transcription(app: AppHandle, mut w: Worker<impl Transcriber + 'static>) {
    thread::spawn(move || {
        let log_path = w.dir.join("transcript.jsonl");
        let mut segments = Vec::new();
        for chunk in &w.chunks {
            match w.transcriber.transcribe(&chunk) {
                Ok(new) => {
                    for segment in new {
                        if let Err(e) = append_jsonl(&log_path, &segment) {
                            tracing::error!("write transcript: {e}");
                        }
                        let _ = w.events.send(TranscriptEvent::Segment {
                            segment: segment.clone(),
                        });
                        segments.push(segment);
                    }
                }
                Err(e) => {
                    let _ = w.events.send(TranscriptEvent::Error {
                        message: e.to_string(),
                    });
                }
            }
        }

        w.session.duration_ms = w
            .duration
            .recv()
            .unwrap_or_else(|_| segments.iter().map(|s| s.end).max().unwrap_or(0));
        let saved = save_session(&app, &w.session, &segments);
        if let Err(message) = &saved {
            let _ = w.events.send(TranscriptEvent::Error {
                message: message.clone(),
            });
        }
        let _ = w.events.send(TranscriptEvent::Finished { session: saved.ok() });
    });
}

fn save_session(app: &AppHandle, session: &Session, segments: &[Segment]) -> Result<SessionRecord, String> {
    let vault = app.state::<SettingsState>().0.lock().unwrap().vault_path.clone();
    let path = murmur_store::write_session_file(&vault, session, segments)
        .map_err(|e| format!("could not write the session file: {e}"))?;
    let file_path = path.to_string_lossy().into_owned();
    app.state::<LibraryState>()
        .0
        .lock()
        .unwrap()
        .save_session(session, segments, &file_path)
        .map_err(|e| format!("could not index the session: {e}"))?;
    Ok(SessionRecord {
        session: session.clone(),
        file_path,
    })
}

fn append_jsonl(path: &Path, segment: &Segment) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, segment)?;
    file.write_all(b"\n")
}

#[tauri::command]
pub fn set_recording_paused(paused: bool, state: State<RecordingState>) -> Result<(), String> {
    let active = state.active.lock().unwrap();
    active
        .as_ref()
        .ok_or("not recording")?
        .recorder
        .set_paused(paused);
    Ok(())
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle) -> Result<Recording, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let active = app.state::<RecordingState>().active.lock().unwrap().take();
        let ActiveSession {
            recorder,
            duration_tx,
        } = active.ok_or("not recording")?;
        let recording = recorder.stop().map_err(|e| e.to_string())?;
        let duration = recording.tracks.iter().map(|t| t.duration_ms).max().unwrap_or(0);
        let _ = duration_tx.send(duration);
        Ok(recording)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn recording_status(state: State<RecordingState>) -> Option<RecorderStatus> {
    state.active.lock().unwrap().as_ref().map(|a| a.recorder.status())
}
