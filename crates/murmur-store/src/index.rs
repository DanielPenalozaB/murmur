use std::path::Path;

use murmur_core::{Channel, ExportFormat, Segment, Session};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::StoreError;

/// A session as listed in the library, with where its file was written.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionRecord {
    #[serde(flatten)]
    pub session: Session,
    pub file_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchHit {
    pub session_id: String,
    pub title: String,
    pub folder: String,
    pub started_at: String,
    /// Segment start within the session, in ms.
    pub start: u64,
    /// Matching text with hits wrapped in `[` `]`.
    pub snippet: String,
}

pub struct Index {
    db: Connection,
}

const SCHEMA: &str = "
CREATE TABLE sessions (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    folder      TEXT NOT NULL,
    started_at  TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    format      TEXT NOT NULL,
    file_path   TEXT NOT NULL
);
CREATE TABLE segments (
    id         INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    start_ms   INTEGER NOT NULL,
    end_ms     INTEGER NOT NULL,
    channel    TEXT NOT NULL,
    speaker    TEXT,
    text       TEXT NOT NULL
);
CREATE INDEX segments_by_session ON segments(session_id, start_ms);
-- External-content FTS table: the text lives in `segments`, triggers keep it in sync.
CREATE VIRTUAL TABLE segments_fts USING fts5(text, content='segments', content_rowid='id', tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER segments_ai AFTER INSERT ON segments BEGIN
    INSERT INTO segments_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER segments_ad AFTER DELETE ON segments BEGIN
    INSERT INTO segments_fts(segments_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
";

impl Index {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(db: Connection) -> Result<Self, StoreError> {
        db.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        // `user_version` tracks applied migrations; add new ones as further steps.
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 1 {
            db.execute_batch(&format!("BEGIN; {SCHEMA} PRAGMA user_version = 1; COMMIT;"))?;
        }
        if version < 2 {
            db.execute_batch(
                "BEGIN; ALTER TABLE sessions ADD COLUMN notes TEXT NOT NULL DEFAULT ''; PRAGMA user_version = 2; COMMIT;",
            )?;
        }
        Ok(Index { db })
    }

    /// Inserts or replaces a session and all its segments.
    pub fn save_session(
        &mut self,
        session: &Session,
        notes: &str,
        segments: &[Segment],
        file_path: &str,
    ) -> Result<(), StoreError> {
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM sessions WHERE id = ?1", [&session.id])?;
        tx.execute(
            "INSERT INTO sessions (id, title, folder, started_at, duration_ms, format, file_path, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                session.id,
                session.title,
                session.folder,
                session.started_at,
                session.duration_ms as i64,
                format_str(session.format),
                file_path,
                notes
            ],
        )?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO segments (session_id, start_ms, end_ms, channel, speaker, text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for s in segments {
                insert.execute(params![
                    session.id,
                    s.start as i64,
                    s.end as i64,
                    channel_str(s.channel),
                    s.speaker.as_ref().map(|id| id.0.as_str()),
                    s.text
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn notes(&self, id: &str) -> Result<String, StoreError> {
        let notes = self
            .db
            .query_row("SELECT notes FROM sessions WHERE id = ?1", [id], |r| r.get(0))
            .optional()?;
        Ok(notes.unwrap_or_default())
    }

    pub fn set_notes(&self, id: &str, notes: &str) -> Result<(), StoreError> {
        self.db
            .execute("UPDATE sessions SET notes = ?2 WHERE id = ?1", params![id, notes])?;
        Ok(())
    }

    /// Sessions newest first, optionally limited to one folder (subfolders excluded).
    pub fn list_sessions(&self, folder: Option<&str>) -> Result<Vec<SessionRecord>, StoreError> {
        let sql = "SELECT id, title, folder, started_at, duration_ms, format, file_path FROM sessions
                   WHERE ?1 IS NULL OR folder = ?1 ORDER BY started_at DESC";
        let mut stmt = self.db.prepare(sql)?;
        let rows = stmt.query_map([folder], row_to_record)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn get_session(&self, id: &str) -> Result<Option<(SessionRecord, Vec<Segment>)>, StoreError> {
        let record = self
            .db
            .query_row(
                "SELECT id, title, folder, started_at, duration_ms, format, file_path FROM sessions WHERE id = ?1",
                [id],
                row_to_record,
            )
            .optional()?;
        let Some(record) = record else { return Ok(None) };
        let mut stmt = self.db.prepare(
            "SELECT start_ms, end_ms, channel, speaker, text FROM segments WHERE session_id = ?1 ORDER BY start_ms",
        )?;
        let segments = stmt
            .query_map([id], |r| {
                Ok(Segment {
                    start: r.get::<_, i64>(0)? as u64,
                    end: r.get::<_, i64>(1)? as u64,
                    channel: parse_channel(&r.get::<_, String>(2)?),
                    speaker: r.get::<_, Option<String>>(3)?.map(murmur_core::SpeakerId),
                    text: r.get(4)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(Some((record, segments)))
    }

    /// Number of sessions per folder, for the sidebar.
    pub fn folder_counts(&self) -> Result<Vec<(String, u32)>, StoreError> {
        let mut stmt = self
            .db
            .prepare("SELECT folder, COUNT(*) FROM sessions GROUP BY folder")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Full-text search over transcripts. Each word is matched as a prefix, so
    /// "deploy" finds "deployment". Accents are ignored.
    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>, StoreError> {
        let Some(fts_query) = to_fts_query(query) else {
            return Ok(Vec::new());
        };
        let mut stmt = self.db.prepare(
            "SELECT s.id, s.title, s.folder, s.started_at, g.start_ms,
                    snippet(segments_fts, 0, '[', ']', '…', 12)
             FROM segments_fts
             JOIN segments g ON g.id = segments_fts.rowid
             JOIN sessions s ON s.id = g.session_id
             WHERE segments_fts MATCH ?1
             ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fts_query, limit], |r| {
            Ok(SearchHit {
                session_id: r.get(0)?,
                title: r.get(1)?,
                folder: r.get(2)?,
                started_at: r.get(3)?,
                start: r.get::<_, i64>(4)? as u64,
                snippet: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

/// Turns free text into a safe FTS5 query: each word quoted (so `-`, `:` or `"` in
/// user input can't break the syntax) and matched as a prefix.
fn to_fts_query(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

fn row_to_record(r: &rusqlite::Row) -> rusqlite::Result<SessionRecord> {
    Ok(SessionRecord {
        session: Session {
            id: r.get(0)?,
            title: r.get(1)?,
            folder: r.get(2)?,
            started_at: r.get(3)?,
            duration_ms: r.get::<_, i64>(4)? as u64,
            format: if r.get::<_, String>(5)? == "txt" {
                ExportFormat::Txt
            } else {
                ExportFormat::Markdown
            },
        },
        file_path: r.get(6)?,
    })
}

fn format_str(f: ExportFormat) -> &'static str {
    match f {
        ExportFormat::Markdown => "markdown",
        ExportFormat::Txt => "txt",
    }
}

fn channel_str(c: Channel) -> &'static str {
    match c {
        Channel::Mic => "mic",
        Channel::System => "system",
    }
}

fn parse_channel(s: &str) -> Channel {
    if s == "mic" { Channel::Mic } else { Channel::System }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, folder: &str, started_at: &str) -> Session {
        Session {
            id: id.into(),
            title: format!("Meeting {id}"),
            folder: folder.into(),
            started_at: started_at.into(),
            duration_ms: 60_000,
            format: ExportFormat::Markdown,
        }
    }

    fn seg(start: u64, text: &str) -> Segment {
        Segment {
            start,
            end: start + 1000,
            channel: Channel::System,
            speaker: None,
            text: text.into(),
        }
    }

    fn index() -> Index {
        let mut idx = Index::open_in_memory().unwrap();
        idx.save_session(
            &session("a", "Work", "2026-10-01T10:00:00Z"),
            "",
            &[
                seg(0, "We moved the deployment to Friday."),
                seg(2000, "Ana owns the rollout plan."),
            ],
            "/v/Work/a.md",
        )
        .unwrap();
        idx.save_session(
            &session("b", "Personal", "2026-10-05T10:00:00Z"),
            "",
            &[seg(0, "Revisamos la migración de la base de datos.")],
            "/v/Personal/b.md",
        )
        .unwrap();
        idx
    }

    #[test]
    fn lists_newest_first_and_filters_by_folder() {
        let idx = index();
        let all: Vec<_> = idx
            .list_sessions(None)
            .unwrap()
            .into_iter()
            .map(|r| r.session.id)
            .collect();
        assert_eq!(all, ["b", "a"]);
        assert_eq!(idx.list_sessions(Some("Work")).unwrap().len(), 1);
        let mut counts = idx.folder_counts().unwrap();
        counts.sort();
        assert_eq!(counts, [("Personal".to_string(), 1), ("Work".to_string(), 1)]);
    }

    #[test]
    fn round_trips_segments() {
        let (record, segments) = index().get_session("a").unwrap().unwrap();
        assert_eq!(record.file_path, "/v/Work/a.md");
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[1].text, "Ana owns the rollout plan.");
        assert!(index().get_session("missing").unwrap().is_none());
    }

    #[test]
    fn searches_by_prefix_ignoring_accents_and_syntax() {
        let idx = index();
        let hits = idx.search("deploy", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, "a");
        assert!(hits[0].snippet.contains("[deployment]"));

        assert_eq!(idx.search("migracion", 10).unwrap()[0].session_id, "b");
        assert!(idx.search("\"-:(", 10).unwrap().is_empty());
        assert!(idx.search("", 10).unwrap().is_empty());
    }

    #[test]
    fn saving_again_replaces_segments() {
        let mut idx = index();
        idx.save_session(
            &session("a", "Work", "2026-10-01T10:00:00Z"),
            "",
            &[seg(0, "Only line.")],
            "/v/a.md",
        )
        .unwrap();
        assert_eq!(idx.get_session("a").unwrap().unwrap().1.len(), 1);
        assert!(idx.search("deployment", 10).unwrap().is_empty());
    }

    #[test]
    fn stores_notes() {
        let idx = index();
        assert_eq!(idx.notes("a").unwrap(), "");
        idx.set_notes("a", "## Summary").unwrap();
        assert_eq!(idx.notes("a").unwrap(), "## Summary");
        assert_eq!(idx.notes("missing").unwrap(), "");
    }

    #[test]
    fn migrates_a_version_1_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let db = Connection::open(&path).unwrap();
            db.execute_batch(&format!("{SCHEMA} PRAGMA user_version = 1;"))
                .unwrap();
        }
        let idx = Index::open(&path).unwrap();
        assert_eq!(idx.notes("none").unwrap(), "");
    }
}
