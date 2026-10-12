//! Persistence. Markdown/TXT files in the vault folder are the user-facing source
//! of truth; SQLite (added in milestone 3) is a rebuildable index for search.

use murmur_core::{ExportFormat, Session};

pub fn file_extension(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Markdown => "md",
        ExportFormat::Txt => "txt",
    }
}

/// Renders the session file. `notes` and `transcript` are already-formatted text.
pub fn render_session_file(session: &Session, notes: &str, transcript: &str, format: ExportFormat) -> String {
    match format {
        ExportFormat::Markdown => {
            let speakers: Vec<&str> = session.speakers.iter().map(|s| s.label.as_str()).collect();
            format!(
                "---\ntitle: {:?}\ndate: {}\nfolder: {:?}\nspeakers: {:?}\n---\n\n# {}\n\n## Notes\n\n{}\n\n## Transcript\n\n{}",
                session.title,
                session.started_at,
                session.folder,
                speakers,
                session.title,
                notes.trim(),
                transcript
            )
        }
        ExportFormat::Txt => format!(
            "{}\n{}\n\nNOTES\n\n{}\n\nTRANSCRIPT\n\n{}",
            session.title,
            session.started_at,
            notes.trim(),
            transcript
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use murmur_core::SessionState;

    #[test]
    fn markdown_has_frontmatter_and_sections() {
        let s = Session {
            id: "1".into(),
            title: "Standup".into(),
            folder: "Work".into(),
            started_at: "2026-10-12T10:00:00Z".into(),
            state: SessionState::Done,
            speakers: vec![],
        };
        let md = render_session_file(&s, "- shipped", "[00:01] Me: hi\n", ExportFormat::Markdown);
        assert!(md.starts_with("---\ntitle: \"Standup\"\n"));
        assert!(md.contains("## Notes\n\n- shipped\n\n## Transcript\n\n[00:01] Me: hi\n"));
    }
}
