use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use murmur_audio::{ChunkSenders, Recorder, RecorderStatus, Recording, SpeechChunk};
use murmur_core::{Channel as AudioChannel, ExportFormat, Segment, Session};
use murmur_store::{SessionRecord, SessionSpeaker, SpeakerInfo};
use murmur_stt::{MeetingTranscriber, Transcriber, WhisperModel};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::library::LibraryState;
use crate::meeting;
use crate::settings::{self, SettingsState};

const WHISPER_MODEL: &str = "ggml-large-v3-turbo-q5_0.bin";
/// Small and fast: draft text while someone is talking, and language ID in meetings
/// that mix languages.
const BASE_MODEL: &str = "ggml-base.bin";

/// Each language is one more candidate for every check, so keep meetings to a few.
pub(crate) const MAX_LANGUAGES: usize = 4;

#[derive(Default)]
pub struct RecordingState {
    active: Mutex<Option<ActiveSession>>,
    starting: AtomicBool,
    /// Loaded at launch and kept for the app's lifetime.
    model: Mutex<Option<Arc<WhisperModel>>>,
    /// Loaded at launch too; drafts and language ID share it.
    base_model: Mutex<Option<Arc<WhisperModel>>>,
    /// The current (or last) recording's transcript so far, for Ask during a meeting.
    live: Arc<Mutex<Vec<Segment>>>,
}

impl RecordingState {
    pub fn live_segments(&self) -> Vec<Segment> {
        self.live.lock().unwrap().clone()
    }
}

struct ActiveSession {
    recorder: Recorder,
    /// Saved with the audio so a re-transcription can default to them.
    languages: Vec<String>,
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
    /// What's being said right now, from the fast model. Replaced by the final
    /// segments once the speaker pauses.
    Draft {
        channel: AudioChannel,
        start: u64,
        text: String,
    },
    /// Final segments up to `until` (ms) are out; drafts starting before it are stale.
    DraftDone {
        channel: AudioChannel,
        until: u64,
    },
    Error {
        message: String,
    },
    /// A step after Stop that takes a while, e.g. identifying speakers.
    Status {
        message: String,
    },
    /// Everything is transcribed and the session file is written and indexed. Carries
    /// the final segments, now labelled with speakers.
    Finished {
        session: Option<SessionRecord>,
        segments: Vec<Segment>,
        speakers: Vec<SpeakerInfo>,
    },
}

/// The `audiocap` sidecar sits next to the app binary (Tauri copies `externalBin` there).
pub(crate) fn helper_path() -> Result<PathBuf, String> {
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
        .map_err(|e| format!("{e}. Download the models from the Murmur setup screen."))?;
    *slot = Some(model.clone());
    Ok(model)
}

/// The transcription model (turbo) and the fast model (base), loaded once.
pub(crate) fn models(app: &AppHandle) -> Result<(Arc<WhisperModel>, Arc<WhisperModel>), String> {
    let state = app.state::<RecordingState>();
    Ok((
        load_model(app, &state.model, WHISPER_MODEL)?,
        load_model(app, &state.base_model, BASE_MODEL)?,
    ))
}

/// Whether a recording is running or starting.
pub(crate) fn is_recording(app: &AppHandle) -> bool {
    let state = app.state::<RecordingState>();
    state.active.lock().unwrap().is_some() || state.starting.load(Ordering::Acquire)
}

/// The final transcriber (turbo) and the draft transcriber (base), both following the
/// meeting's languages.
fn transcribers(
    app: &AppHandle,
    languages: &[String],
) -> Result<(MeetingTranscriber, MeetingTranscriber), String> {
    let (model, base) = models(app)?;
    let terms = vocabulary(app);
    let mut finals = MeetingTranscriber::new(&model, Some(&base), languages).map_err(|e| e.to_string())?;
    let mut drafts = MeetingTranscriber::new(&base, Some(&base), languages).map_err(|e| e.to_string())?;
    finals.set_vocabulary(&terms);
    drafts.set_vocabulary(&terms);
    Ok((finals, drafts))
}

/// The user's vocabulary plus the names of remembered speakers.
pub(crate) fn vocabulary(app: &AppHandle) -> Vec<String> {
    let mut terms = app.state::<SettingsState>().0.lock().unwrap().vocabulary.clone();
    match app.state::<LibraryState>().0.lock().unwrap().voices() {
        Ok(voices) => terms.extend(voices.into_iter().map(|(name, _)| name)),
        Err(e) => tracing::warn!("could not read remembered speakers: {e}"),
    }
    terms
}

/// Loads the models in the background at launch so the first Start is instant.
pub fn preload_model(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        let languages = app.state::<SettingsState>().0.lock().unwrap().languages.clone();
        match transcribers(&app, &languages) {
            // The first inference compiles GPU kernels (~2 s); do it now, not mid-meeting.
            Ok((mut finals, mut drafts)) => {
                let silence = SpeechChunk {
                    channel: AudioChannel::Mic,
                    start_ms: 0,
                    samples: vec![0.0; murmur_audio::SAMPLE_RATE as usize],
                };
                let _ = drafts.transcribe(&silence);
                let _ = finals.transcribe(&silence);
            }
            Err(e) => tracing::warn!("model preload failed: {e}"),
        }
    });
}

/// Loads the local LLM while the meeting starts, so the first question is answered
/// as fast as the rest instead of waiting 10–20 s for the model to load.
fn warm_up_llm(app: &AppHandle) {
    let provider = app.state::<SettingsState>().0.lock().unwrap().llm.clone();
    tauri::async_runtime::spawn(async move {
        let started = std::time::Instant::now();
        match murmur_llm::warm_up(&provider).await {
            Ok(()) => tracing::info!("{} ready in {:?}", provider.label(), started.elapsed()),
            Err(e) => tracing::warn!("could not preload {}: {e}", provider.label()),
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
    meeting::clear(&app);
    warm_up_llm(&app);
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
            let (transcriber, drafter) = transcribers(&app, &languages)?;
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
            let (drafts_tx, drafts_rx) = mpsc::channel();
            let senders = ChunkSenders {
                finals: chunks_tx,
                drafts: Some(drafts_tx),
            };
            let recorder =
                Recorder::start(&helper_path()?, &dir, Some(senders)).map_err(|e| e.to_string())?;
            let (duration_tx, duration_rx) = mpsc::channel();
            state.live.lock().unwrap().clear();
            let finalized = Arc::new(Finalized::default());
            spawn_drafts(drafter, drafts_rx, finalized.clone(), on_event.clone());
            spawn_transcription(
                app.clone(),
                Worker {
                    session,
                    transcriber,
                    chunks: chunks_rx,
                    duration: duration_rx,
                    live: state.live.clone(),
                    finalized,
                    dir,
                    events: on_event,
                },
            );
            *state.active.lock().unwrap() = Some(ActiveSession {
                recorder,
                languages: languages.clone(),
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
    live: Arc<Mutex<Vec<Segment>>>,
    finalized: Arc<Finalized>,
    /// Session audio folder; `transcript.jsonl` is appended here as segments arrive.
    dir: PathBuf,
    events: Channel<TranscriptEvent>,
}

/// Per channel, where final transcription has reached (ms). Drafts of audio before
/// that are stale.
#[derive(Default)]
struct Finalized([AtomicU64; 2]);

impl Finalized {
    fn slot(&self, channel: AudioChannel) -> &AtomicU64 {
        &self.0[match channel {
            AudioChannel::Mic => 0,
            AudioChannel::System => 1,
        }]
    }

    fn set(&self, channel: AudioChannel, until: u64) {
        self.slot(channel).fetch_max(until, Ordering::Relaxed);
    }

    fn is_stale(&self, chunk: &SpeechChunk) -> bool {
        chunk.start_ms < self.slot(chunk.channel).load(Ordering::Relaxed)
    }
}

/// Transcribes in-progress speech with the fast model so text appears while people
/// talk. Runs beside the final worker so drafts never delay final lines, and only
/// drafts the newest audio per channel when it falls behind.
fn spawn_drafts(
    mut drafter: impl Transcriber + 'static,
    drafts: Receiver<SpeechChunk>,
    finalized: Arc<Finalized>,
    events: Channel<TranscriptEvent>,
) {
    thread::spawn(move || {
        while let Ok(first) = drafts.recv() {
            // Keep only the latest draft per channel; older ones are already outdated.
            let mut latest: Vec<SpeechChunk> = vec![first];
            for chunk in drafts.try_iter() {
                latest.retain(|c| c.channel != chunk.channel);
                latest.push(chunk);
            }
            for chunk in latest {
                if finalized.is_stale(&chunk) {
                    continue;
                }
                let text = match drafter.transcribe(&chunk) {
                    Ok(segments) => segments
                        .iter()
                        .map(|s| s.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                    Err(e) => {
                        tracing::debug!("draft failed: {e}");
                        continue;
                    }
                };
                if !text.is_empty() && !finalized.is_stale(&chunk) {
                    let _ = events.send(TranscriptEvent::Draft {
                        channel: chunk.channel,
                        start: chunk.start_ms,
                        text,
                    });
                }
            }
        }
    });
}

/// Transcribes chunks in arrival order until the recorder drops its senders on stop,
/// then writes the session file into the vault, indexes it and reports `Finished`.
fn spawn_transcription(app: AppHandle, mut w: Worker<impl Transcriber + 'static>) {
    thread::spawn(move || {
        let log_path = w.dir.join("transcript.jsonl");
        let mut segments = Vec::new();
        for chunk in &w.chunks {
            let result = w.transcriber.transcribe(&chunk);
            let until =
                chunk.start_ms + (chunk.samples.len() as u64 * 1000 / murmur_audio::SAMPLE_RATE as u64);
            w.finalized.set(chunk.channel, until);
            match result {
                Ok(new) => {
                    for segment in new {
                        if let Err(e) = append_jsonl(&log_path, &segment) {
                            tracing::error!("write transcript: {e}");
                        }
                        let _ = w.events.send(TranscriptEvent::Segment {
                            segment: segment.clone(),
                        });
                        w.live.lock().unwrap().push(segment.clone());
                        segments.push(segment);
                    }
                }
                Err(e) => {
                    let _ = w.events.send(TranscriptEvent::Error {
                        message: e.to_string(),
                    });
                }
            }
            let _ = w.events.send(TranscriptEvent::DraftDone {
                channel: chunk.channel,
                until,
            });
        }

        w.session.duration_ms = w
            .duration
            .recv()
            .unwrap_or_else(|_| segments.iter().map(|s| s.end).max().unwrap_or(0));
        if !segments.is_empty() {
            let _ = w.events.send(TranscriptEvent::Status {
                message: "Identifying speakers…".into(),
            });
        }
        let speakers = crate::speakers::identify(&app, &w.dir, &mut segments).unwrap_or_else(|e| {
            // The transcript is still worth saving with "Them" labels.
            tracing::warn!("speaker identification failed: {e}");
            let _ = w.events.send(TranscriptEvent::Error {
                message: format!("Couldn't tell speakers apart: {e}"),
            });
            Vec::new()
        });
        // Speakers were added and lines with no voice removed.
        if let Err(e) = rewrite_jsonl(&log_path, &segments) {
            tracing::error!("rewrite transcript: {e}");
        }
        let saved = save_session(&app, &w.session, &segments, &speakers);
        if let Err(message) = &saved {
            let _ = w.events.send(TranscriptEvent::Error {
                message: message.clone(),
            });
        }
        let speakers = speakers
            .iter()
            .map(|s| SpeakerInfo {
                id: s.id.clone(),
                name: s
                    .name
                    .clone()
                    .unwrap_or_else(|| murmur_core::SpeakerId(s.id.clone()).default_name()),
                named: s.name.is_some(),
            })
            .collect();
        let _ = w.events.send(TranscriptEvent::Finished {
            session: saved.ok(),
            segments,
            speakers,
        });
    });
}

fn save_session(
    app: &AppHandle,
    session: &Session,
    segments: &[Segment],
    speakers: &[SessionSpeaker],
) -> Result<SessionRecord, String> {
    let vault = app.state::<SettingsState>().0.lock().unwrap().vault_path.clone();
    let names = crate::speakers::names_of(speakers);
    let path = murmur_store::write_session_file(&vault, session, "", segments, &names)
        .map_err(|e| format!("could not write the session file: {e}"))?;
    let file_path = path.to_string_lossy().into_owned();
    let library = app.state::<LibraryState>();
    let mut index = library.0.lock().unwrap();
    index
        .save_session(session, "", segments, &file_path)
        .map_err(|e| format!("could not index the session: {e}"))?;
    index
        .save_speakers(&session.id, speakers)
        .map_err(|e| format!("could not save speakers: {e}"))?;
    Ok(SessionRecord {
        session: session.clone(),
        file_path,
    })
}

/// Replaces the audio folder's transcript log with the speaker-labelled segments.
pub(crate) fn rewrite_jsonl(path: &Path, segments: &[Segment]) -> std::io::Result<()> {
    let mut out = Vec::new();
    for s in segments {
        serde_json::to_writer(&mut out, s)?;
        out.push(b'\n');
    }
    std::fs::write(path, out)
}

/// `audio.json` beside the tracks: each track's volume boost, keyed by channel, and
/// the meeting's languages.
fn write_audio_info(recording: &Recording, languages: &[String]) -> std::io::Result<()> {
    let gains: serde_json::Map<String, serde_json::Value> = recording
        .tracks
        .iter()
        .map(|t| {
            let channel = match t.channel {
                AudioChannel::Mic => "mic",
                AudioChannel::System => "system",
            };
            (channel.to_string(), serde_json::json!(t.gain))
        })
        .collect();
    let info = serde_json::json!({ "gains": gains, "languages": languages });
    std::fs::write(
        recording.dir.join("audio.json"),
        serde_json::to_vec_pretty(&info)?,
    )
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

/// Stops listening to the user's microphone (e.g. while muted in the call) or resumes.
#[tauri::command]
pub fn set_mic_muted(muted: bool, state: State<RecordingState>) -> Result<(), String> {
    let active = state.active.lock().unwrap();
    active
        .as_ref()
        .ok_or("not recording")?
        .recorder
        .set_muted(AudioChannel::Mic, muted);
    Ok(())
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle) -> Result<Recording, String> {
    meeting::clear(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let active = app.state::<RecordingState>().active.lock().unwrap().take();
        let ActiveSession {
            recorder,
            languages,
            duration_tx,
        } = active.ok_or("not recording")?;
        let recording = recorder.stop().map_err(|e| e.to_string())?;
        // Lets a re-transcription undo the volume boost and use the same languages.
        if let Err(e) = write_audio_info(&recording, &languages) {
            tracing::warn!("write audio.json: {e}");
        }
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
