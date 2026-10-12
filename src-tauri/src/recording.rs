use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use murmur_audio::{Recorder, RecorderStatus, Recording, SpeechChunk};
use murmur_core::Segment;
use murmur_stt::{Transcriber, WhisperModel};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

const WHISPER_MODEL: &str = "ggml-large-v3-turbo-q5_0.bin";

#[derive(Default)]
pub struct RecordingState {
    recorder: Mutex<Option<Recorder>>,
    starting: AtomicBool,
    /// Loaded on the first recording and kept for the app's lifetime.
    model: Mutex<Option<Arc<WhisperModel>>>,
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
    /// All audio up to Stop has been transcribed.
    Finished,
}

/// The `audiocap` sidecar sits next to the app binary (Tauri copies `externalBin` there).
fn helper_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(exe.with_file_name("audiocap"))
}

fn load_model(app: &AppHandle) -> Result<Arc<WhisperModel>, String> {
    let state = app.state::<RecordingState>();
    let mut slot = state.model.lock().unwrap();
    if let Some(model) = slot.as_ref() {
        return Ok(model.clone());
    }
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models")
        .join(WHISPER_MODEL);
    let model = WhisperModel::load(&path)
        .map_err(|e| format!("{e}. Run scripts/download-models.sh to download it."))?;
    *slot = Some(model.clone());
    Ok(model)
}

/// Loads Whisper in the background at launch so the first Start is instant.
pub fn preload_model(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = load_model(&app) {
            tracing::warn!("model preload failed: {e}");
        }
    });
}

/// Async so the slow parts (model load, helper startup) run off the main thread and
/// the window stays responsive.
#[tauri::command]
pub async fn start_recording(app: AppHandle, on_event: Channel<TranscriptEvent>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<RecordingState>();
        // Guards against a second click while this start is still in progress. The
        // recorder mutex is only held briefly, so status polling never blocks.
        if state.recorder.lock().unwrap().is_some()
            || state
                .starting
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return Err("a recording is already running".into());
        }
        let result = (|| {
            let transcriber = load_model(&app)?.transcriber(None).map_err(|e| e.to_string())?;
            let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
            let dir = app
                .path()
                .app_data_dir()
                .map_err(|e| e.to_string())?
                .join("sessions")
                .join(stamp);
            let (chunks_tx, chunks_rx) = mpsc::channel();
            let recorder =
                Recorder::start(&helper_path()?, &dir, Some(chunks_tx)).map_err(|e| e.to_string())?;
            spawn_transcription(transcriber, chunks_rx, dir.join("transcript.jsonl"), on_event);
            *state.recorder.lock().unwrap() = Some(recorder);
            Ok(())
        })();
        state.starting.store(false, Ordering::Release);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Transcribes chunks in arrival order until the recorder drops its senders on stop,
/// then reports `Finished`. Segments are appended to `transcript.jsonl` as they come.
fn spawn_transcription(
    mut transcriber: impl Transcriber + 'static,
    chunks: Receiver<SpeechChunk>,
    log_path: PathBuf,
    events: Channel<TranscriptEvent>,
) {
    thread::spawn(move || {
        for chunk in chunks {
            match transcriber.transcribe(&chunk) {
                Ok(segments) => {
                    for segment in segments {
                        if let Err(e) = append_jsonl(&log_path, &segment) {
                            tracing::error!("write transcript: {e}");
                        }
                        let _ = events.send(TranscriptEvent::Segment { segment });
                    }
                }
                Err(e) => {
                    let _ = events.send(TranscriptEvent::Error {
                        message: e.to_string(),
                    });
                }
            }
        }
        let _ = events.send(TranscriptEvent::Finished);
    });
}

fn append_jsonl(path: &Path, segment: &Segment) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, segment)?;
    file.write_all(b"\n")
}

#[tauri::command]
pub fn set_recording_paused(paused: bool, state: State<RecordingState>) -> Result<(), String> {
    let slot = state.recorder.lock().unwrap();
    let recorder = slot.as_ref().ok_or("not recording")?;
    recorder.set_paused(paused);
    Ok(())
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle) -> Result<Recording, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let recorder = app.state::<RecordingState>().recorder.lock().unwrap().take();
        recorder.ok_or("not recording")?.stop().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn recording_status(state: State<RecordingState>) -> Option<RecorderStatus> {
    state.recorder.lock().unwrap().as_ref().map(Recorder::status)
}
