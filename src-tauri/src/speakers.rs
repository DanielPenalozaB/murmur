//! Speakers on the system-audio channel: identified after each meeting, renamed by the
//! user, and remembered by voice so they're named automatically next time.

use std::path::Path;
use std::sync::Mutex;

use std::collections::HashSet;

use murmur_core::{Channel, Segment, SpeakerId, SpeakerNames};
use murmur_store::{Index, NamedSpeaker, SessionRecord, SessionSpeaker, SpeakerInfo, StoreError, VoiceEntry};
use murmur_stt::review::{self, Finding, Labelled};
use murmur_stt::speakers::{self, Diarizer, VOICE_MATCH_THRESHOLD};
use serde::Serialize;
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

/// Renames a speaker in one session: rewrites the transcript in the session file and
/// remembers the voice under the new name. Naming a "Speaker N" also replaces it in
/// the notes; correcting a wrong name leaves the notes alone, since they may mention
/// the real person by that name, and takes the voice out of the old name's.
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
        .ok_or("speaker not found")?;
    if old.named && old.name == name {
        return Ok(());
    }
    let voiceprint = index
        .rename_speaker(&session_id, &speaker_id, &name)
        .map_err(|e| e.to_string())?;
    index
        .remember_voice(&name, &voiceprint)
        .map_err(|e| e.to_string())?;
    if old.named {
        index.rebuild_voice(&old.name).map_err(|e| e.to_string())?;
    }
    let path = rewrite_transcript(&index, &session_id)?;

    if !old.named {
        let notes = index.notes(&session_id).map_err(|e| e.to_string())?;
        let renamed = rename_in_notes(&notes, &old.name, &name);
        if renamed != notes {
            let format = index
                .get_session(&session_id)
                .map_err(|e| e.to_string())?
                .ok_or("session not found")?
                .0
                .session
                .format;
            murmur_store::write_notes(&path, format, &renamed)
                .map_err(|e| format!("could not update the notes: {e}"))?;
            index
                .set_notes(&session_id, &renamed)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Takes a wrong name off a speaker ("Speaker N" again) and out of that person's
/// remembered voice. The notes are left alone, as when correcting a name.
#[tauri::command]
pub fn unname_speaker(app: AppHandle, session_id: String, speaker_id: String) -> Result<(), String> {
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    let old = index
        .unname_speaker(&session_id, &speaker_id)
        .map_err(|e| e.to_string())?;
    let Some(old) = old else {
        return Ok(());
    };
    index.rebuild_voice(&old).map_err(|e| e.to_string())?;
    rewrite_transcript(&index, &session_id)?;
    Ok(())
}

/// Writes the session file's transcript with the current speaker names; returns
/// the file's path.
fn rewrite_transcript(index: &Index, session_id: &str) -> Result<std::path::PathBuf, String> {
    let (record, segments) = index
        .get_session(session_id)
        .map_err(|e| e.to_string())?
        .ok_or("session not found")?;
    let names = index.speaker_names(session_id).map_err(|e| e.to_string())?;
    let path = std::path::PathBuf::from(&record.file_path);
    let format = record.session.format;
    let body = murmur_store::render_transcript(format, &segments, &names);
    murmur_store::write_transcript(&path, format, &body)
        .map_err(|e| format!("could not update the file: {e}"))?;
    Ok(path)
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

/// A named speaker in a review finding, with what's needed to show and open it.
#[derive(Debug, Serialize)]
pub struct ReviewSpeaker {
    session_id: String,
    title: String,
    started_at: String,
    speaker_id: String,
    /// "Speaker 10", what the speaker is called once unnamed.
    label: String,
}

impl From<&NamedSpeaker> for ReviewSpeaker {
    fn from(s: &NamedSpeaker) -> Self {
        ReviewSpeaker {
            session_id: s.session_id.clone(),
            title: s.title.clone(),
            started_at: s.started_at.clone(),
            speaker_id: s.speaker_id.clone(),
            label: SpeakerId(s.speaker_id.clone()).default_name(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Compared {
    speaker: ReviewSpeaker,
    score: f32,
}

/// Something to double-check about speaker names. `key` identifies it for "Looks right".
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewItem {
    /// The only two speakers with this name sound like different people.
    Pair {
        key: String,
        name: String,
        score: f32,
        speakers: [ReviewSpeaker; 2],
    },
    /// One speaker sounds unlike the others with this name.
    Outlier {
        key: String,
        name: String,
        score: f32,
        speaker: ReviewSpeaker,
        compared: Vec<Compared>,
    },
    /// Two names whose voices sound like one person.
    LookAlike {
        key: String,
        names: [String; 2],
        score: f32,
    },
}

impl ReviewItem {
    fn key(&self) -> &str {
        match self {
            ReviewItem::Pair { key, .. }
            | ReviewItem::Outlier { key, .. }
            | ReviewItem::LookAlike { key, .. } => key,
        }
    }
}

fn speaker_key(s: &NamedSpeaker) -> String {
    format!("{}/{}", s.session_id, s.speaker_id)
}

/// Named speakers that may have the wrong name, and names that may be one person.
pub fn review(index: &Index) -> Result<Vec<ReviewItem>, StoreError> {
    let named = index.named_speakers()?;
    let labelled: Vec<_> = named
        .iter()
        .map(|s| Labelled {
            name: &s.name,
            voiceprint: &s.voiceprint,
        })
        .collect();
    let mut items: Vec<ReviewItem> = review::doubtful_names(&labelled)
        .into_iter()
        .filter_map(|finding| match finding {
            Finding::Pair { a, b, score } => Some(ReviewItem::Pair {
                key: format!(
                    "pair:{}:{}:{}",
                    named[a].name,
                    speaker_key(&named[a]),
                    speaker_key(&named[b])
                ),
                name: named[a].name.clone(),
                score,
                speakers: [(&named[a]).into(), (&named[b]).into()],
            }),
            Finding::Outlier {
                speaker,
                score,
                compared,
            } => Some(ReviewItem::Outlier {
                key: format!("outlier:{}:{}", named[speaker].name, speaker_key(&named[speaker])),
                name: named[speaker].name.clone(),
                score,
                speaker: (&named[speaker]).into(),
                compared: compared
                    .into_iter()
                    .map(|(j, score)| Compared {
                        speaker: (&named[j]).into(),
                        score,
                    })
                    .collect(),
            }),
            Finding::LookAlike { .. } => None,
        })
        .collect();

    let met: HashSet<(&str, &str)> = named
        .iter()
        .map(|s| (s.session_id.as_str(), s.name.as_str()))
        .collect();
    let voices = index.voices()?;
    let together = |a: &str, b: &str| {
        met.iter()
            .any(|&(session, name)| name == a && met.contains(&(session, b)))
    };
    for finding in review::look_alikes(&voices, together) {
        if let Finding::LookAlike { a, b, score } = finding {
            let mut names = [voices[a].0.clone(), voices[b].0.clone()];
            names.sort();
            items.push(ReviewItem::LookAlike {
                key: format!("alike:{}|{}", names[0], names[1]),
                names,
                score,
            });
        }
    }

    let dismissed: HashSet<String> = index.dismissed_reviews()?.into_iter().collect();
    items.retain(|item| !dismissed.contains(item.key()));
    Ok(items)
}

#[tauri::command]
pub fn speaker_review(app: AppHandle) -> Result<Vec<ReviewItem>, String> {
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    review(&index).map_err(|e| e.to_string())
}

/// "Looks right": stops showing a review finding.
#[tauri::command]
pub fn dismiss_review(app: AppHandle, key: String) -> Result<(), String> {
    let library = app.state::<LibraryState>();
    let index = library.0.lock().unwrap();
    index.dismiss_review(&key).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use murmur_core::{ExportFormat, Session};

    use super::*;

    fn meeting(index: &mut Index, id: &str, speakers: &[(&str, &str, f32)]) {
        let session = Session {
            id: id.into(),
            title: format!("Meeting {id}"),
            folder: String::new(),
            started_at: format!("2026-10-0{id}T10:00:00Z"),
            duration_ms: 60_000,
            format: ExportFormat::Markdown,
        };
        index.save_session(&session, "", &[], "/v/m.md").unwrap();
        let speakers: Vec<_> = speakers
            .iter()
            .map(|(speaker, name, angle)| SessionSpeaker {
                id: speaker.to_string(),
                name: Some(name.to_string()),
                voiceprint: vec![angle.to_radians().cos(), angle.to_radians().sin()],
            })
            .collect();
        index.save_speakers(id, &speakers).unwrap();
    }

    #[test]
    fn reviews_names_and_forgets_dismissed_findings() {
        let mut index = Index::open_in_memory().unwrap();
        meeting(
            &mut index,
            "1",
            &[("speaker_9", "Ruben", 0.0), ("speaker_10", "Ruben", 80.0)],
        );
        meeting(&mut index, "2", &[("speaker_1", "Ana", 40.0)]);
        index.remember_voice("Ana", &[1.0, 0.0]).unwrap();
        index.remember_voice("Ann", &[0.99, 0.141]).unwrap();

        let items = review(&index).unwrap();
        let keys: Vec<_> = items.iter().map(|i| i.key().to_string()).collect();
        assert_eq!(keys, ["pair:Ruben:1/speaker_9:1/speaker_10", "alike:Ana|Ann"]);
        let ReviewItem::Pair { speakers, .. } = &items[0] else {
            panic!("{items:?}");
        };
        assert_eq!(speakers[1].label, "Speaker 10");

        index.dismiss_review("alike:Ana|Ann").unwrap();
        assert_eq!(review(&index).unwrap().len(), 1);
    }

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
