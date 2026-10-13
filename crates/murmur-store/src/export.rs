use std::path::{Path, PathBuf};

use murmur_core::{Channel, ExportFormat, Segment, Session, SpeakerId, SpeakerNames, speaker_label};

use crate::{StoreError, validate_folder};

pub fn file_extension(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Markdown => "md",
        ExportFormat::Txt => "txt",
    }
}

/// Consecutive segments from one speaker, merged into a paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub channel: Channel,
    pub speaker: Option<SpeakerId>,
    pub start: u64,
    pub text: String,
}

/// Same rule as the UI: same channel and speaker, and a gap of at most 4 s, means one
/// turn.
pub fn group_turns(segments: &[Segment]) -> Vec<Turn> {
    let mut sorted: Vec<&Segment> = segments.iter().collect();
    sorted.sort_by_key(|s| s.start);
    let mut turns: Vec<Turn> = Vec::new();
    let mut last_end = 0;
    for s in sorted {
        match turns.last_mut() {
            Some(t)
                if t.channel == s.channel
                    && t.speaker == s.speaker
                    && s.start.saturating_sub(last_end) <= 4_000 =>
            {
                t.text.push(' ');
                t.text.push_str(s.text.trim());
            }
            _ => turns.push(Turn {
                channel: s.channel,
                speaker: s.speaker.clone(),
                start: s.start,
                text: s.text.trim().to_string(),
            }),
        }
        last_end = last_end.max(s.end);
    }
    turns
}

fn timestamp(ms: u64) -> String {
    let secs = ms / 1000;
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    } else {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

/// `2026-10-12 Sprint review.md`. Characters that are unsafe in file names are removed.
pub fn file_name(session: &Session) -> String {
    let date = session.started_at.get(..10).unwrap_or(&session.started_at);
    let title: String = session
        .title
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '-'
            } else {
                c
            }
        })
        .collect();
    let title = title.trim().trim_start_matches('.');
    let title = if title.is_empty() { "Meeting" } else { title };
    format!("{date} {title}.{}", file_extension(session.format))
}

/// Markdown notes sit between these markers (invisible when rendered), so they can be
/// replaced without touching the transcript or anything else edited in the file.
const NOTES_START: &str = "<!-- murmur:notes -->";
const NOTES_END: &str = "<!-- /murmur:notes -->";
/// In plain text the transcript heading closes the notes.
const TXT_TRANSCRIPT: &str = "TRANSCRIPT\n\n";
const MD_TRANSCRIPT: &str = "## Transcript\n\n";

fn notes_block(format: ExportFormat, notes: &str) -> String {
    let notes = notes.trim();
    match format {
        ExportFormat::Markdown if notes.is_empty() => format!("{NOTES_START}\n{NOTES_END}\n\n"),
        ExportFormat::Markdown => format!("{NOTES_START}\n{notes}\n{NOTES_END}\n\n"),
        ExportFormat::Txt if notes.is_empty() => String::new(),
        ExportFormat::Txt => format!("NOTES\n\n{notes}\n\n"),
    }
}

pub fn render_session_file(
    session: &Session,
    notes: &str,
    segments: &[Segment],
    names: &SpeakerNames,
) -> String {
    let duration = timestamp(session.duration_ms);
    let header = match session.format {
        ExportFormat::Markdown => format!(
            "---\ntitle: {}\ndate: {}\nduration: \"{}\"\nfolder: {}\nsession: {}\n---\n\n# {}\n\n{}{MD_TRANSCRIPT}",
            yaml_string(&session.title),
            session.started_at,
            duration,
            yaml_string(&session.folder),
            session.id,
            session.title,
            notes_block(session.format, notes),
        ),
        ExportFormat::Txt => format!(
            "{}\n{} · {}\n\n{}{TXT_TRANSCRIPT}",
            session.title,
            session.started_at,
            duration,
            notes_block(session.format, notes),
        ),
    };
    header + &render_transcript(session.format, segments, names)
}

/// The transcript section's body: one paragraph per turn.
pub fn render_transcript(format: ExportFormat, segments: &[Segment], names: &SpeakerNames) -> String {
    let mut out = String::new();
    for t in group_turns(segments) {
        let who = speaker_label(t.channel, t.speaker.as_ref(), names);
        match format {
            ExportFormat::Markdown => {
                out.push_str(&format!("**{who}** · {}  \n{}\n\n", timestamp(t.start), t.text))
            }
            ExportFormat::Txt => out.push_str(&format!("[{}] {who}: {}\n\n", timestamp(t.start), t.text)),
        }
    }
    out
}

/// Replaces everything after the transcript heading, e.g. after renaming a speaker.
/// Notes and anything above the transcript stay as they are.
pub fn replace_transcript(text: &str, format: ExportFormat, body: &str) -> Result<String, StoreError> {
    let heading = match format {
        ExportFormat::Markdown => MD_TRANSCRIPT,
        ExportFormat::Txt => TXT_TRANSCRIPT,
    };
    let at = text.rfind(heading).ok_or(StoreError::Unrecognized)? + heading.len();
    Ok(format!("{}{body}", &text[..at]))
}

pub fn write_transcript(path: &Path, format: ExportFormat, body: &str) -> Result<(), StoreError> {
    let text = std::fs::read_to_string(path)?;
    std::fs::write(path, replace_transcript(&text, format, body)?)?;
    Ok(())
}

/// JSON strings are valid YAML scalars and escape everything that needs it.
fn yaml_string(s: &str) -> String {
    serde_json::to_string(s).expect("strings always serialize")
}

/// Writes the session file into `<vault>/<folder>/`, never overwriting an existing
/// file (adds " 2", " 3"… instead). Returns the path written.
pub fn write_session_file(
    vault: &Path,
    session: &Session,
    notes: &str,
    segments: &[Segment],
    names: &SpeakerNames,
) -> Result<PathBuf, StoreError> {
    validate_folder(&session.folder)?;
    let dir = vault.join(&session.folder);
    std::fs::create_dir_all(&dir)?;
    let name = file_name(session);
    let ext = file_extension(session.format);
    let stem = name.strip_suffix(&format!(".{ext}")).unwrap_or(&name).to_string();
    let mut path = dir.join(&name);
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem} {n}.{ext}"));
        n += 1;
    }
    std::fs::write(&path, render_session_file(session, notes, segments, names))?;
    Ok(path)
}

/// Replaces the notes in an existing session file, leaving everything else as is.
/// Files without a notes area (written before notes existed) get one above the
/// transcript.
pub fn replace_notes(text: &str, format: ExportFormat, notes: &str) -> Result<String, StoreError> {
    let block = notes_block(format, notes);
    let (start, end) = match format {
        ExportFormat::Markdown => match (text.find(NOTES_START), text.find(NOTES_END)) {
            (Some(start), Some(end)) if end > start => {
                let mut end = end + NOTES_END.len();
                // Swallow the blank line the block ends with, since `block` adds it back.
                while text[end..].starts_with('\n') {
                    end += 1;
                }
                (start, end)
            }
            _ => {
                let at = text.find("## Transcript").ok_or(StoreError::Unrecognized)?;
                (at, at)
            }
        },
        ExportFormat::Txt => {
            let transcript = text
                .find(&format!("\n{TXT_TRANSCRIPT}"))
                .ok_or(StoreError::Unrecognized)?
                + 1;
            // Notes, if any, start after the two header lines and a blank line.
            let header_end = text.find("\n\n").map_or(0, |i| i + 2).min(transcript);
            (header_end, transcript)
        }
    };
    Ok(format!("{}{block}{}", &text[..start], &text[end..]))
}

pub fn write_notes(path: &Path, format: ExportFormat, notes: &str) -> Result<(), StoreError> {
    let text = std::fs::read_to_string(path)?;
    std::fs::write(path, replace_notes(&text, format, notes)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(format: ExportFormat) -> Session {
        Session {
            id: "2026-10-12_10-00-00".into(),
            title: "Sprint: review / Q4".into(),
            folder: "Work".into(),
            started_at: "2026-10-12T10:00:00-05:00".into(),
            duration_ms: 125_000,
            format,
        }
    }

    fn seg(start: u64, channel: Channel, text: &str) -> Segment {
        Segment {
            start,
            end: start + 1500,
            channel,
            speaker: None,
            text: text.into(),
        }
    }

    fn spoken_by(mut s: Segment, id: &str) -> Segment {
        s.speaker = Some(SpeakerId(id.into()));
        s
    }

    fn segments() -> Vec<Segment> {
        vec![
            seg(65_000, Channel::Mic, "Sounds good."),
            seg(1_000, Channel::System, "Hi all."),
            seg(3_000, Channel::System, "Let's start."),
        ]
    }

    #[test]
    fn groups_turns_in_time_order() {
        let turns = group_turns(&segments());
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].text, "Hi all. Let's start.");
        assert_eq!(turns[1].channel, Channel::Mic);
    }

    #[test]
    fn file_name_is_dated_and_safe() {
        assert_eq!(
            file_name(&session(ExportFormat::Markdown)),
            "2026-10-12 Sprint- review - Q4.md"
        );
        let mut s = session(ExportFormat::Txt);
        s.title = "  ".into();
        assert_eq!(file_name(&s), "2026-10-12 Meeting.txt");
    }

    #[test]
    fn markdown_has_frontmatter_and_transcript() {
        let md = render_session_file(
            &session(ExportFormat::Markdown),
            "",
            &segments(),
            &SpeakerNames::new(),
        );
        assert!(md.starts_with(
            "---\ntitle: \"Sprint: review / Q4\"\ndate: 2026-10-12T10:00:00-05:00\nduration: \"02:05\"\n"
        ));
        assert!(md.contains("**Them** · 00:01  \nHi all. Let's start.\n\n**Me** · 01:05  \nSounds good.\n"));
    }

    #[test]
    fn txt_is_plain() {
        let txt = render_session_file(&session(ExportFormat::Txt), "", &segments(), &SpeakerNames::new());
        assert!(txt.contains("[00:01] Them: Hi all. Let's start.\n\n[01:05] Me: Sounds good.\n"));
    }

    #[test]
    fn never_overwrites_existing_files() {
        let vault = tempfile::tempdir().unwrap();
        let s = session(ExportFormat::Markdown);
        let a = write_session_file(vault.path(), &s, "", &segments(), &SpeakerNames::new()).unwrap();
        let b = write_session_file(vault.path(), &s, "", &segments(), &SpeakerNames::new()).unwrap();
        assert_ne!(a, b);
        assert!(b.ends_with("Work/2026-10-12 Sprint- review - Q4 2.md"));
    }

    #[test]
    fn notes_are_replaced_without_touching_the_rest() {
        for format in [ExportFormat::Markdown, ExportFormat::Txt] {
            let s = session(format);
            let empty = render_session_file(&s, "", &segments(), &SpeakerNames::new());
            let with_notes = replace_notes(&empty, format, "## Summary\n- shipped").unwrap();
            assert_eq!(
                with_notes,
                render_session_file(&s, "## Summary\n- shipped", &segments(), &SpeakerNames::new())
            );
            let edited = with_notes.replace("Hi all.", "Hi everyone.");
            let replaced = replace_notes(&edited, format, "- new").unwrap();
            assert!(
                replaced.contains("- new") && !replaced.contains("shipped"),
                "{replaced}"
            );
            assert!(replaced.contains("Hi everyone."));
            assert!(!replace_notes(&replaced, format, "").unwrap().contains("- new"));
        }
    }

    #[test]
    fn notes_are_added_to_files_written_before_notes_existed() {
        let old = "---\ntitle: \"T\"\n---\n\n# T\n\n## Transcript\n\n**Me** · 00:01  \nhi\n";
        let updated = replace_notes(old, ExportFormat::Markdown, "- note").unwrap();
        assert!(
            updated.contains("# T\n\n<!-- murmur:notes -->\n- note\n<!-- /murmur:notes -->\n\n## Transcript")
        );
    }

    #[test]
    fn splits_turns_when_the_speaker_changes() {
        let segments = [
            spoken_by(seg(1_000, Channel::System, "Hi."), "speaker_1"),
            spoken_by(seg(3_000, Channel::System, "Hello."), "speaker_2"),
            spoken_by(seg(5_000, Channel::System, "Let's start."), "speaker_2"),
        ];
        let turns = group_turns(&segments);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[1].text, "Hello. Let's start.");
    }

    #[test]
    fn renders_speaker_names() {
        let segments = [
            spoken_by(seg(1_000, Channel::System, "Hi."), "speaker_1"),
            spoken_by(seg(3_000, Channel::System, "Hello."), "speaker_2"),
        ];
        let names = SpeakerNames::from([("speaker_1".to_string(), "Ana".to_string())]);
        let md = render_transcript(ExportFormat::Markdown, &segments, &names);
        assert_eq!(
            md,
            "**Ana** · 00:01  \nHi.\n\n**Speaker 2** · 00:03  \nHello.\n\n"
        );
    }

    #[test]
    fn transcript_is_replaced_without_touching_notes() {
        for format in [ExportFormat::Markdown, ExportFormat::Txt] {
            let s = session(format);
            let segments = [spoken_by(seg(1_000, Channel::System, "Hi."), "speaker_1")];
            let names = SpeakerNames::from([("speaker_1".to_string(), "Ana".to_string())]);
            let before = render_session_file(&s, "- Ana says hi", &segments, &SpeakerNames::new());
            let body = render_transcript(format, &segments, &names);
            let after = replace_transcript(&before, format, &body).unwrap();
            assert_eq!(after, render_session_file(&s, "- Ana says hi", &segments, &names));
        }
    }
}
