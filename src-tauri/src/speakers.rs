//! Speakers on the system-audio channel: identified after each meeting, renamed by the
//! user, and remembered by voice so they're named automatically next time.

use std::path::Path;
use std::sync::Mutex;

use murmur_core::{Channel, Segment, SpeakerNames};
use murmur_store::{Index, SessionRecord, SessionSpeaker, SpeakerInfo, StoreError, VoiceEntry};
use murmur_stt::speakers::{self, Diarizer, VOICE_MATCH_THRESHOLD};
use tauri::{AppHandle, Manager};

use crate::library::LibraryState;

/// Loaded on first use (about 50 MB) and kept.
#[derive(Default)]
pub struct SpeakersState(Mutex<Option<Diarizer>>);

/// Finds who spoke when in `<dir>/system.wav`, labels the system-audio segments
/// (`speaker_1`, `speaker_2`…) and names the ones whose voice is remembered. Lines on
/// either track in which the speaker model hears no voice (music, hold tunes, a
/// "Thank you." Whisper made up from mic noise) are removed.
pub fn identify(
    app: &AppHandle,
    dir: &Path,
    segments: &mut Vec<Segment>,
) -> Result<Vec<SessionSpeaker>, String> {
    let state = app.state::<SpeakersState>();
    let mut slot = state.0.lock().unwrap();
    if slot.is_none() {
        let models = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("models")
            .join("speakers");
        let diarizer = Diarizer::load(&models)
            .map_err(|e| format!("{e}. Download the models from the Murmur setup screen."))?;
        *slot = Some(diarizer);
    }
    let diarizer = slot.as_ref().expect("loaded above");

    if segments.iter().any(|s| s.channel == Channel::Mic) {
        let samples = murmur_audio::read_wav(&dir.join("mic.wav")).map_err(|e| e.to_string())?;
        let turns = diarizer.run(&samples).map_err(|e| e.to_string())?.turns;
        let dropped = speakers::drop_unheard(segments, Channel::Mic, &turns);
        tracing::info!("dropped {dropped} mic lines with no voice in them");
    }
    if !segments.iter().any(|s| s.channel == Channel::System) {
        return Ok(Vec::new());
    }
    let samples = murmur_audio::read_wav(&dir.join("system.wav")).map_err(|e| e.to_string())?;
    let started = std::time::Instant::now();
    let diarization = diarizer.run(&samples).map_err(|e| e.to_string())?;
    drop(slot);
    tracing::info!(
        "{} speakers in {:?} for {} s of audio",
        diarization.voiceprints.len(),
        started.elapsed(),
        samples.len() / murmur_audio::SAMPLE_RATE as usize
    );
    let dropped = speakers::drop_unheard(segments, Channel::System, &diarization.turns);
    tracing::info!("dropped {dropped} call-audio lines with no voice in them");

    let voiceprints = speakers::label_segments(segments, &diarization);
    let known = app
        .state::<LibraryState>()
        .0
        .lock()
        .unwrap()
        .voices()
        .map_err(|e| e.to_string())?;
    let names = speakers::match_voices(&voiceprints, &known, VOICE_MATCH_THRESHOLD);
    Ok(voiceprints
        .into_iter()
        .zip(names)
        .enumerate()
        .map(|(i, (voiceprint, name))| SessionSpeaker {
            id: speakers::speaker_id(i).0,
            name,
            voiceprint,
        })
        .collect())
}

pub fn names_of(speakers: &[SessionSpeaker]) -> SpeakerNames {
    speakers
        .iter()
        .filter_map(|s| Some((s.id.clone(), s.name.clone()?)))
        .collect()
}

/// Fills in who unnamed speakers probably are: remembered voices they match that
/// aren't in the meeting already. Mostly speakers from before their voice was named.
pub fn suggest(index: &Index, session_id: &str, speakers: &mut [SpeakerInfo]) -> Result<(), StoreError> {
    let unnamed = index.unnamed_speakers(session_id)?;
    if unnamed.is_empty() {
        return Ok(());
    }
    let known = index.voices()?;
    let taken: Vec<String> = speakers
        .iter()
        .filter(|s| s.named)
        .map(|s| s.name.clone())
        .collect();
    let prints: Vec<_> = unnamed
        .iter()
        .map(|s| (s.voiceprint.clone(), s.declined.clone()))
        .collect();
    let names = speakers::suggest_names(&prints, &known, &taken);
    for (unnamed, name) in unnamed.iter().zip(names) {
        if let Some(speaker) = speakers.iter_mut().find(|s| s.id == unnamed.id) {
            speaker.suggestion = name;
        }
    }
    Ok(())
}

/// "Not them": stops suggesting `name` for this speaker.
#[tauri::command]
pub fn decline_suggestion(
    app: AppHandle,
    session_id: String,
    speaker_id: String,
    name: String,
) -> Result<(), String> {
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    index
        .decline_suggestion(&session_id, &speaker_id, &name)
        .map_err(|e| e.to_string())
}

/// Renames a speaker in one session: rewrites the transcript in the session file,
/// replaces the old name in the notes, and remembers the voice under the new name.
#[tauri::command]
pub fn rename_speaker(
    app: AppHandle,
    session_id: String,
    speaker_id: String,
    name: String,
) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() || name.len() > 80 {
        return Err("enter a name of up to 80 characters".into());
    }
    if name.eq_ignore_ascii_case("me") {
        return Err("\"Me\" is reserved for your microphone".into());
    }
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    let old = index
        .speakers(&session_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.id == speaker_id)
        .ok_or("speaker not found")?
        .name;
    let voiceprint = index
        .rename_speaker(&session_id, &speaker_id, &name)
        .map_err(|e| e.to_string())?;
    index
        .remember_voice(&name, &voiceprint)
        .map_err(|e| e.to_string())?;

    let (record, segments) = index
        .get_session(&session_id)
        .map_err(|e| e.to_string())?
        .ok_or("session not found")?;
    let names = index.speaker_names(&session_id).map_err(|e| e.to_string())?;
    let path = Path::new(&record.file_path);
    let format = record.session.format;
    let body = murmur_store::render_transcript(format, &segments, &names);
    murmur_store::write_transcript(path, format, &body)
        .map_err(|e| format!("could not update the file: {e}"))?;

    let notes = index.notes(&session_id).map_err(|e| e.to_string())?;
    let renamed = rename_in_notes(&notes, &old, &name);
    if renamed != notes {
        murmur_store::write_notes(path, format, &renamed)
            .map_err(|e| format!("could not update the notes: {e}"))?;
        index
            .set_notes(&session_id, &renamed)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Replaces whole-word occurrences of a speaker's old name, so renaming "Speaker 1"
/// leaves "Speaker 12" alone.
fn rename_in_notes(notes: &str, old: &str, new: &str) -> String {
    if old.is_empty() || old == new {
        return notes.to_string();
    }
    let mut out = String::with_capacity(notes.len());
    let mut rest = notes;
    while let Some(at) = rest.find(old) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + old.len()..].chars().next();
        let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
        out.push_str(&rest[..at]);
        out.push_str(if boundary(before) && boundary(after) {
            new
        } else {
            old
        });
        rest = &rest[at + old.len()..];
    }
    out.push_str(rest);
    out
}

#[tauri::command]
pub fn list_voices(app: AppHandle) -> Result<Vec<VoiceEntry>, String> {
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    index.voice_directory().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn forget_voice(app: AppHandle, name: String) -> Result<(), String> {
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    index.forget_voice(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn sessions_with_speaker(app: AppHandle, name: String) -> Result<Vec<SessionRecord>, String> {
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    index.sessions_with_speaker(&name).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::rename_in_notes;

    #[test]
    fn renames_whole_words_only() {
        let notes = "- [ ] Speaker 1: send plan [00:31]\n- Speaker 12 agreed (Speaker 1)";
        assert_eq!(
            rename_in_notes(notes, "Speaker 1", "Ana"),
            "- [ ] Ana: send plan [00:31]\n- Speaker 12 agreed (Ana)"
        );
        assert_eq!(rename_in_notes("Ana and Anabel", "Ana", "Bo"), "Bo and Anabel");
    }
}
