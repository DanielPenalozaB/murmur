//! Runs a recorded session through the transcription pipeline again, so sessions
//! benefit from improvements (or different languages) without recording again. The
//! user's notes are kept; the transcript and speakers are replaced.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use murmur_audio::SpeechChunk;
use murmur_core::{Channel as AudioChannel, Segment};
use murmur_stt::{MeetingTranscriber, Transcriber};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::library::LibraryState;
use crate::recording::{self, MAX_LANGUAGES};

/// One re-transcription at a time; it keeps the GPU busy for minutes.
#[derive(Default)]
pub struct RetranscribeState(AtomicBool);

pub fn is_running(app: &AppHandle) -> bool {
    app.state::<RetranscribeState>().0.load(Ordering::Acquire)
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RetranscribeEvent {
    /// `done` of `total` speech chunks transcribed.
    Progress {
        done: usize,
        total: usize,
    },
    Status {
        message: String,
    },
    Finished {
        segments: usize,
        speakers: usize,
    },
}

/// The languages a session was recorded with, from its `audio.json`.
#[tauri::command]
pub fn session_languages(app: AppHandle, id: String) -> Result<Option<Vec<String>>, String> {
    let info = audio_info(&session_dir(&app, &id)?);
    Ok(info["languages"].as_array().map(|langs| {
        langs
            .iter()
            .filter_map(|l| l.as_str().map(String::from))
            .collect()
    }))
}

#[tauri::command]
pub async fn retranscribe_session(
    app: AppHandle,
    id: String,
    languages: Vec<String>,
    on_event: Channel<RetranscribeEvent>,
) -> Result<(), String> {
    if languages.len() > MAX_LANGUAGES {
        return Err(format!("choose at most {MAX_LANGUAGES} languages"));
    }
    if recording::is_recording(&app) {
        return Err("stop the current recording first".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let busy = &app.state::<RetranscribeState>().0;
        if busy.swap(true, Ordering::AcqRel) {
            return Err("another session is being re-transcribed".into());
        }
        let result = run(&app, &id, &languages, &on_event);
        if let Err(e) = &result {
            tracing::error!(session = %id, "re-transcription failed: {e}");
        }
        busy.store(false, Ordering::Release);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

fn run(
    app: &AppHandle,
    id: &str,
    languages: &[String],
    events: &Channel<RetranscribeEvent>,
) -> Result<(), String> {
    let dir = session_dir(app, id)?;
    // Without them the transcript would be replaced with an empty one.
    if !["mic.wav", "system.wav"].iter().any(|f| dir.join(f).exists()) {
        return Err("this session's recordings were deleted".into());
    }
    let (record, old_segments) = app
        .state::<LibraryState>()
        .0
        .lock()
        .unwrap()
        .get_session(id)
        .map_err(|e| e.to_string())?
        .ok_or("session not found")?;
    let _ = events.send(RetranscribeEvent::Status {
        message: "Loading models…".into(),
    });
    let (turbo, base) = recording::models(app)?;
    let mut transcriber =
        MeetingTranscriber::new(&turbo, Some(&base), languages).map_err(|e| e.to_string())?;
    transcriber.set_vocabulary(&recording::vocabulary(app));

    // Cut both tracks into speech chunks the way the live recorder does, from the
    // audio as it was before the volume boost.
    let info = audio_info(&dir);
    let mut chunks: Vec<SpeechChunk> = Vec::new();
    for (channel, name) in [(AudioChannel::Mic, "mic"), (AudioChannel::System, "system")] {
        let path = dir.join(format!("{name}.wav"));
        if !path.exists() {
            continue;
        }
        let gain = info["gains"][name].as_f64().unwrap_or(1.0) as f32;
        let samples = murmur_audio::read_wav(&path).map_err(|e| e.to_string())?;
        chunks.extend(murmur_audio::speech_chunks(channel, &samples, gain));
    }
    if chunks.is_empty() {
        return Err("this session has no recorded audio to transcribe".into());
    }

    let total = chunks.len();
    let mut segments: Vec<Segment> = Vec::new();
    for (done, chunk) in chunks.iter().enumerate() {
        segments.extend(transcriber.transcribe(chunk).map_err(|e| e.to_string())?);
        let _ = events.send(RetranscribeEvent::Progress {
            done: done + 1,
            total,
        });
    }
    segments.sort_by_key(|s| s.start);

    let _ = events.send(RetranscribeEvent::Status {
        message: "Identifying speakers…".into(),
    });
    let speakers = crate::speakers::identify(app, &dir, &mut segments)?;

    // Keep the first transcript, in case the new one is worse.
    let log = dir.join("transcript.jsonl");
    let original = dir.join("transcript.original.jsonl");
    if !original.exists() {
        if log.exists() {
            std::fs::copy(&log, &original).map_err(|e| e.to_string())?;
        } else {
            recording::rewrite_jsonl(&original, &old_segments).map_err(|e| e.to_string())?;
        }
    }
    recording::rewrite_jsonl(&log, &segments).map_err(|e| e.to_string())?;

    let names = crate::speakers::names_of(&speakers);
    let format = record.session.format;
    let body = murmur_store::render_transcript(format, &segments, &names);
    murmur_store::write_transcript(Path::new(&record.file_path), format, &body)
        .map_err(|e| format!("could not update the file: {e}"))?;
    let library = app.state::<LibraryState>();
    let mut index = library.0.lock().unwrap();
    let notes = index.notes(id).map_err(|e| e.to_string())?;
    index
        .save_session(&record.session, &notes, &segments, &record.file_path)
        .map_err(|e| e.to_string())?;
    index.save_speakers(id, &speakers).map_err(|e| e.to_string())?;

    let _ = events.send(RetranscribeEvent::Finished {
        segments: segments.len(),
        speakers: speakers.len(),
    });
    Ok(())
}

pub(crate) fn session_dir(app: &AppHandle, id: &str) -> Result<std::path::PathBuf, String> {
    // Session ids are timestamps; refuse anything that could leave the folder.
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("invalid session id".into());
    }
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("sessions")
        .join(id))
}

/// `audio.json` written when the recording stopped; `null` for older sessions.
fn audio_info(dir: &Path) -> serde_json::Value {
    std::fs::read(dir.join("audio.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(serde_json::Value::Null)
}
