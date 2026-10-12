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

pub fn render_session_file(session: &Session, segments: &[Segment]) -> String {
    let turns = group_turns(segments);
    let duration = timestamp(session.duration_ms);
    match session.format {
        ExportFormat::Markdown => {
            let mut out = format!(
                "---\ntitle: {}\ndate: {}\nduration: \"{}\"\nfolder: {}\nsession: {}\n---\n\n# {}\n\n## Transcript\n\n",
                yaml_string(&session.title),
                session.started_at,
                duration,
                yaml_string(&session.folder),
                session.id,
                session.title,
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
                "{}\n{} · {}\n\nTRANSCRIPT\n\n",
                session.title, session.started_at, duration
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
    std::fs::write(&path, render_session_file(session, segments))?;
    Ok(path)
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
        let md = render_session_file(&session(ExportFormat::Markdown), &segments());
        assert!(md.starts_with(
            "---\ntitle: \"Sprint: review / Q4\"\ndate: 2026-10-12T10:00:00-05:00\nduration: \"02:05\"\n"
        ));
        assert!(md.contains("**Them** · 00:01  \nHi all. Let's start.\n\n**Me** · 01:05  \nSounds good.\n"));
    }

    #[test]
    fn txt_is_plain() {
        let txt = render_session_file(&session(ExportFormat::Txt), &segments());
        assert!(txt.contains("[00:01] Them: Hi all. Let's start.\n\n[01:05] Me: Sounds good.\n"));
    }

    #[test]
    fn never_overwrites_existing_files() {
        let vault = tempfile::tempdir().unwrap();
        let s = session(ExportFormat::Markdown);
        let a = write_session_file(vault.path(), &s, &segments()).unwrap();
        let b = write_session_file(vault.path(), &s, &segments()).unwrap();
        assert_ne!(a, b);
        assert!(b.ends_with("Work/2026-10-12 Sprint- review - Q4 2.md"));
    }
}
