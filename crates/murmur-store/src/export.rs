use std::path::{Path, PathBuf};

use murmur_core::{Channel, ExportFormat, Segment, Session};

use crate::{StoreError, validate_folder};

pub fn file_extension(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Markdown => "md",
        ExportFormat::Txt => "txt",
    }
}

/// Consecutive segments from one channel, merged into a paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub channel: Channel,
    pub start: u64,
    pub text: String,
}

/// Same rule as the UI: same channel and a gap of at most 4 s means one turn.
pub fn group_turns(segments: &[Segment]) -> Vec<Turn> {
    let mut sorted: Vec<&Segment> = segments.iter().collect();
    sorted.sort_by_key(|s| s.start);
    let mut turns: Vec<Turn> = Vec::new();
    let mut last_end = 0;
    for s in sorted {
        match turns.last_mut() {
            Some(t) if t.channel == s.channel && s.start.saturating_sub(last_end) <= 4_000 => {
                t.text.push(' ');
                t.text.push_str(s.text.trim());
            }
            _ => turns.push(Turn {
                channel: s.channel,
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

fn notes_block(format: ExportFormat, notes: &str) -> String {
    let notes = notes.trim();
    match format {
        ExportFormat::Markdown if notes.is_empty() => format!("{NOTES_START}\n{NOTES_END}\n\n"),
        ExportFormat::Markdown => format!("{NOTES_START}\n{notes}\n{NOTES_END}\n\n"),
        ExportFormat::Txt if notes.is_empty() => String::new(),
        ExportFormat::Txt => format!("NOTES\n\n{notes}\n\n"),
    }
}

pub fn render_session_file(session: &Session, notes: &str, segments: &[Segment]) -> String {
    let turns = group_turns(segments);
    let duration = timestamp(session.duration_ms);
    match session.format {
        ExportFormat::Markdown => {
            let mut out = format!(
                "---\ntitle: {}\ndate: {}\nduration: \"{}\"\nfolder: {}\nsession: {}\n---\n\n# {}\n\n{}## Transcript\n\n",
                yaml_string(&session.title),
                session.started_at,
                duration,
                yaml_string(&session.folder),
                session.id,
                session.title,
                notes_block(session.format, notes),
            );
            for t in &turns {
                out.push_str(&format!(
                    "**{}** · {}  \n{}\n\n",
                    t.channel.label(),
                    timestamp(t.start),
                    t.text
                ));
            }
            out
        }
        ExportFormat::Txt => {
            let mut out = format!(
                "{}\n{} · {}\n\n{}{TXT_TRANSCRIPT}",
                session.title,
                session.started_at,
                duration,
                notes_block(session.format, notes),
            );
            for t in &turns {
                out.push_str(&format!(
                    "[{}] {}: {}\n\n",
                    timestamp(t.start),
                    t.channel.label(),
                    t.text
                ));
            }
            out
        }
    }
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
    std::fs::write(&path, render_session_file(session, notes, segments))?;
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
        let md = render_session_file(&session(ExportFormat::Markdown), "", &segments());
        assert!(md.starts_with(
            "---\ntitle: \"Sprint: review / Q4\"\ndate: 2026-10-12T10:00:00-05:00\nduration: \"02:05\"\n"
        ));
        assert!(md.contains("**Them** · 00:01  \nHi all. Let's start.\n\n**Me** · 01:05  \nSounds good.\n"));
    }

    #[test]
    fn txt_is_plain() {
        let txt = render_session_file(&session(ExportFormat::Txt), "", &segments());
        assert!(txt.contains("[00:01] Them: Hi all. Let's start.\n\n[01:05] Me: Sounds good.\n"));
    }

    #[test]
    fn never_overwrites_existing_files() {
        let vault = tempfile::tempdir().unwrap();
        let s = session(ExportFormat::Markdown);
        let a = write_session_file(vault.path(), &s, "", &segments()).unwrap();
        let b = write_session_file(vault.path(), &s, "", &segments()).unwrap();
        assert_ne!(a, b);
        assert!(b.ends_with("Work/2026-10-12 Sprint- review - Q4 2.md"));
    }

    #[test]
    fn notes_are_replaced_without_touching_the_rest() {
        for format in [ExportFormat::Markdown, ExportFormat::Txt] {
            let s = session(format);
            let empty = render_session_file(&s, "", &segments());
            let with_notes = replace_notes(&empty, format, "## Summary\n- shipped").unwrap();
            assert_eq!(
                with_notes,
                render_session_file(&s, "## Summary\n- shipped", &segments())
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
}
