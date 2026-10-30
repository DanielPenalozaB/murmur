use std::path::Path;

use murmur_core::{Channel, ExportFormat, Segment, Session, SpeakerId, SpeakerNames};
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

/// A session as shown on a library card.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionCard {
    #[serde(flatten)]
    pub record: SessionRecord,
    /// First summary bullet, or the start of the transcript when there are no notes.
    pub excerpt: String,
    pub actions_open: u32,
    pub actions_done: u32,
    /// Names of the people on the call, in speaking order.
    pub speakers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchHit {
    pub session_id: String,
    pub title: String,
    pub folder: String,
    pub started_at: String,
    /// Segment start within the session, in ms.
    pub start: u64,
    /// Who said it: "Me", a speaker's name, "Speaker 2" or "Them".
    pub speaker: String,
    /// Matching text with hits wrapped in `[` `]`.
    pub snippet: String,
}

/// A speaker found in one session's system audio.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSpeaker {
    /// `speaker_1`, `speaker_2`…
    pub id: String,
    /// Set when the user named them, or when their voice matched a remembered one.
    pub name: Option<String>,
    /// L2-normalized embedding; empty when there wasn't enough audio.
    pub voiceprint: Vec<f32>,
}

/// A speaker as shown in the app.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpeakerInfo {
    pub id: String,
    /// The name, or "Speaker N" until they're named.
    pub name: String,
    pub named: bool,
}

/// A remembered voice, for the speaker directory.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VoiceEntry {
    pub name: String,
    /// Meetings this person was identified in.
    pub meetings: u32,
    /// `started_at` of the latest of those meetings.
    pub last_seen: Option<String>,
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

const SPEAKERS_SCHEMA: &str = "
CREATE TABLE session_speakers (
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    speaker_id TEXT NOT NULL,
    name       TEXT,
    voiceprint BLOB NOT NULL,
    PRIMARY KEY (session_id, speaker_id)
);
-- One averaged voiceprint per named person, used to name them in later meetings.
CREATE TABLE voices (
    name       TEXT PRIMARY KEY,
    voiceprint BLOB NOT NULL,
    samples    INTEGER NOT NULL
);
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
        if version < 3 {
            db.execute_batch(&format!(
                "BEGIN; {SPEAKERS_SCHEMA} PRAGMA user_version = 3; COMMIT;"
            ))?;
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

    /// Replaces the speakers found in a session.
    pub fn save_speakers(&mut self, session_id: &str, speakers: &[SessionSpeaker]) -> Result<(), StoreError> {
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM session_speakers WHERE session_id = ?1", [session_id])?;
        for s in speakers {
            tx.execute(
                "INSERT INTO session_speakers (session_id, speaker_id, name, voiceprint) VALUES (?1, ?2, ?3, ?4)",
                params![session_id, s.id, s.name, to_blob(&s.voiceprint)],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Speakers of a session in order (Speaker 1 first).
    pub fn speakers(&self, session_id: &str) -> Result<Vec<SpeakerInfo>, StoreError> {
        let mut stmt = self.db.prepare(
            "SELECT speaker_id, name FROM session_speakers WHERE session_id = ?1
             ORDER BY CAST(SUBSTR(speaker_id, 9) AS INTEGER)",
        )?;
        let rows = stmt.query_map([session_id], |r| {
            let id: String = r.get(0)?;
            let name: Option<String> = r.get(1)?;
            Ok(SpeakerInfo {
                name: name
                    .clone()
                    .unwrap_or_else(|| SpeakerId(id.clone()).default_name()),
                named: name.is_some(),
                id,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Names the user (or voice matching) gave this session's speakers.
    pub fn speaker_names(&self, session_id: &str) -> Result<SpeakerNames, StoreError> {
        Ok(self
            .speakers(session_id)?
            .into_iter()
            .filter(|s| s.named)
            .map(|s| (s.id, s.name))
            .collect())
    }

    /// Names a session's speaker and returns their voiceprint (empty if unknown).
    pub fn rename_speaker(
        &self,
        session_id: &str,
        speaker_id: &str,
        name: &str,
    ) -> Result<Vec<f32>, StoreError> {
        let changed = self.db.execute(
            "UPDATE session_speakers SET name = ?3 WHERE session_id = ?1 AND speaker_id = ?2",
            params![session_id, speaker_id, name],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("speaker {speaker_id}")));
        }
        let blob: Vec<u8> = self.db.query_row(
            "SELECT voiceprint FROM session_speakers WHERE session_id = ?1 AND speaker_id = ?2",
            params![session_id, speaker_id],
            |r| r.get(0),
        )?;
        Ok(from_blob(&blob))
    }

    /// Adds a voiceprint to a person's averaged voice, creating it if new.
    pub fn remember_voice(&self, name: &str, voiceprint: &[f32]) -> Result<(), StoreError> {
        if voiceprint.is_empty() {
            return Ok(());
        }
        let existing: Option<(Vec<u8>, i64)> = self
            .db
            .query_row(
                "SELECT voiceprint, samples FROM voices WHERE name = ?1",
                [name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (merged, samples) = match existing {
            Some((blob, n)) => {
                let old = from_blob(&blob);
                if old.len() != voiceprint.len() {
                    (voiceprint.to_vec(), 1)
                } else {
                    let sum: Vec<f32> = old
                        .iter()
                        .zip(voiceprint)
                        .map(|(a, b)| a * n as f32 + b)
                        .collect();
                    (normalized(sum), n + 1)
                }
            }
            None => (voiceprint.to_vec(), 1),
        };
        self.db.execute(
            "INSERT INTO voices (name, voiceprint, samples) VALUES (?1, ?2, ?3)
             ON CONFLICT(name) DO UPDATE SET voiceprint = excluded.voiceprint, samples = excluded.samples",
            params![name, to_blob(&merged), samples],
        )?;
        Ok(())
    }

    /// Every remembered voice, for matching new meetings' speakers.
    pub fn voices(&self) -> Result<Vec<(String, Vec<f32>)>, StoreError> {
        let mut stmt = self
            .db
            .prepare("SELECT name, voiceprint FROM voices ORDER BY name")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, from_blob(&r.get::<_, Vec<u8>>(1)?)))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Remembered voices with how many meetings each person was in.
    pub fn voice_directory(&self) -> Result<Vec<VoiceEntry>, StoreError> {
        let mut stmt = self.db.prepare(
            "SELECT v.name, COUNT(DISTINCT ss.session_id), MAX(s.started_at)
             FROM voices v
             LEFT JOIN session_speakers ss ON ss.name = v.name
             LEFT JOIN sessions s ON s.id = ss.session_id
             GROUP BY v.name ORDER BY COUNT(DISTINCT ss.session_id) DESC, v.name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(VoiceEntry {
                name: r.get(0)?,
                meetings: r.get(1)?,
                last_seen: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Forgets a voice so it's no longer matched. Past sessions keep the name.
    pub fn forget_voice(&self, name: &str) -> Result<(), StoreError> {
        self.db.execute("DELETE FROM voices WHERE name = ?1", [name])?;
        Ok(())
    }

    /// Sessions a named person spoke in, newest first.
    pub fn sessions_with_speaker(&self, name: &str) -> Result<Vec<SessionRecord>, StoreError> {
        let mut stmt = self.db.prepare(
            "SELECT DISTINCT s.id, s.title, s.folder, s.started_at, s.duration_ms, s.format, s.file_path
             FROM sessions s JOIN session_speakers ss ON ss.session_id = s.id
             WHERE ss.name = ?1 ORDER BY s.started_at DESC",
        )?;
        let rows = stmt.query_map([name], row_to_record)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Sessions newest first, optionally limited to one folder (subfolders excluded).
    pub fn list_sessions(&self, folder: Option<&str>) -> Result<Vec<SessionRecord>, StoreError> {
        let sql = "SELECT id, title, folder, started_at, duration_ms, format, file_path FROM sessions
                   WHERE ?1 IS NULL OR folder = ?1 ORDER BY started_at DESC";
        let mut stmt = self.db.prepare(sql)?;
        let rows = stmt.query_map([folder], row_to_record)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Sessions in `folder` and its subfolders.
    pub fn sessions_under(&self, folder: &str) -> Result<Vec<SessionRecord>, StoreError> {
        // A prefix compare rather than LIKE, which would treat `_` and `%` in names
        // as wildcards.
        let mut stmt = self.db.prepare(
            "SELECT id, title, folder, started_at, duration_ms, format, file_path FROM sessions
             WHERE folder = ?1 OR substr(folder, 1, length(?1) + 1) = ?1 || '/'",
        )?;
        let rows = stmt.query_map([folder], row_to_record)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn relocate_session(
        &self,
        id: &str,
        title: &str,
        folder: &str,
        file_path: &str,
    ) -> Result<(), StoreError> {
        self.db.execute(
            "UPDATE sessions SET title = ?2, folder = ?3, file_path = ?4 WHERE id = ?1",
            params![id, title, folder, file_path],
        )?;
        Ok(())
    }

    /// Removes a session with its transcript and speakers. Remembered voices stay.
    pub fn delete_session(&self, id: &str) -> Result<(), StoreError> {
        self.db.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Sessions newest first with what a library card shows.
    pub fn session_cards(&self, folder: Option<&str>) -> Result<Vec<SessionCard>, StoreError> {
        let records = self.list_sessions(folder)?;
        let mut cards = Vec::with_capacity(records.len());
        let mut first_line = self
            .db
            .prepare("SELECT text FROM segments WHERE session_id = ?1 ORDER BY start_ms LIMIT 1")?;
        for record in records {
            let id = &record.session.id;
            let notes = self.notes(id)?;
            let (actions_open, actions_done) = crate::count_actions(&notes);
            let mut excerpt = crate::excerpt(&notes);
            if excerpt.is_empty() {
                excerpt = first_line
                    .query_row([id], |r| r.get::<_, String>(0))
                    .optional()?
                    .unwrap_or_default();
            }
            let speakers = self.speakers(id)?.into_iter().map(|s| s.name).collect();
            cards.push(SessionCard {
                record,
                excerpt,
                actions_open,
                actions_done,
                speakers,
            });
        }
        Ok(cards)
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
                    snippet(segments_fts, 0, '[', ']', '…', 12), g.channel, g.speaker, ss.name
             FROM segments_fts
             JOIN segments g ON g.id = segments_fts.rowid
             JOIN sessions s ON s.id = g.session_id
             LEFT JOIN session_speakers ss ON ss.session_id = g.session_id AND ss.speaker_id = g.speaker
             WHERE segments_fts MATCH ?1
             ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fts_query, limit], |r| {
            let channel = parse_channel(&r.get::<_, String>(6)?);
            let speaker = r.get::<_, Option<String>>(7)?.map(SpeakerId);
            let names: SpeakerNames = match (&speaker, r.get::<_, Option<String>>(8)?) {
                (Some(id), Some(name)) => SpeakerNames::from([(id.0.clone(), name)]),
                _ => SpeakerNames::new(),
            };
            Ok(SearchHit {
                session_id: r.get(0)?,
                title: r.get(1)?,
                folder: r.get(2)?,
                started_at: r.get(3)?,
                start: r.get::<_, i64>(4)? as u64,
                speaker: murmur_core::speaker_label(channel, speaker.as_ref(), &names),
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

fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn from_blob(b: &[u8]) -> Vec<f32> {
    b.as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}

fn normalized(mut v: Vec<f32>) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
    v
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

    fn speaker(id: &str, name: Option<&str>, voiceprint: Vec<f32>) -> SessionSpeaker {
        SessionSpeaker {
            id: id.into(),
            name: name.map(String::from),
            voiceprint,
        }
    }

    #[test]
    fn stores_and_renames_session_speakers() {
        let mut idx = index();
        idx.save_speakers(
            "a",
            &[
                speaker("speaker_2", None, vec![0.0, 1.0]),
                speaker("speaker_1", Some("Ana"), vec![1.0, 0.0]),
                speaker("speaker_10", None, vec![]),
            ],
        )
        .unwrap();
        let names: Vec<_> = idx.speakers("a").unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["Ana", "Speaker 2", "Speaker 10"], "numeric order");
        assert_eq!(idx.speaker_names("a").unwrap().len(), 1);

        assert_eq!(
            idx.rename_speaker("a", "speaker_2", "Bo").unwrap(),
            vec![0.0, 1.0]
        );
        assert_eq!(idx.speaker_names("a").unwrap()["speaker_2"], "Bo");
        assert!(idx.rename_speaker("a", "speaker_9", "X").is_err());
    }

    #[test]
    fn averages_remembered_voices() {
        let mut idx = index();
        idx.remember_voice("Ana", &[1.0, 0.0]).unwrap();
        idx.remember_voice("Ana", &[0.0, 1.0]).unwrap();
        idx.remember_voice("Ignored", &[]).unwrap();
        let voices = idx.voices().unwrap();
        assert_eq!(voices.len(), 1);
        let v = &voices[0].1;
        assert!(
            (v[0] - v[1]).abs() < 1e-6 && (v[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3,
            "{v:?}"
        );

        idx.save_speakers("a", &[speaker("speaker_1", Some("Ana"), vec![1.0, 0.0])])
            .unwrap();
        idx.save_speakers("b", &[speaker("speaker_1", Some("Ana"), vec![1.0, 0.0])])
            .unwrap();
        let dir = idx.voice_directory().unwrap();
        assert_eq!(dir[0].meetings, 2);
        assert_eq!(dir[0].last_seen.as_deref(), Some("2026-10-05T10:00:00Z"));
        assert_eq!(idx.sessions_with_speaker("Ana").unwrap().len(), 2);

        idx.forget_voice("Ana").unwrap();
        assert!(idx.voices().unwrap().is_empty());
    }

    #[test]
    fn cards_summarize_notes_and_speakers() {
        let mut idx = index();
        idx.set_notes(
            "a",
            "## Summary\n- Deploy Friday [00:01]\n## Action items\n- [ ] Ana: plan",
        )
        .unwrap();
        idx.save_speakers("a", &[speaker("speaker_1", Some("Ana"), vec![])])
            .unwrap();
        let cards = idx.session_cards(None).unwrap();
        let a = cards.iter().find(|c| c.record.session.id == "a").unwrap();
        assert_eq!(a.excerpt, "Deploy Friday");
        assert_eq!((a.actions_open, a.actions_done), (1, 0));
        assert_eq!(a.speakers, ["Ana"]);
        let b = cards.iter().find(|c| c.record.session.id == "b").unwrap();
        assert_eq!(
            b.excerpt, "Revisamos la migración de la base de datos.",
            "falls back to the transcript"
        );
    }

    #[test]
    fn search_hits_say_who_spoke() {
        let mut idx = Index::open_in_memory().unwrap();
        let mut ana = seg(0, "The deployment moves to Friday.");
        ana.speaker = Some(SpeakerId("speaker_1".into()));
        let mut me = seg(2_000, "Deployment checklist is ready.");
        me.channel = Channel::Mic;
        idx.save_session(
            &session("a", "Work", "2026-10-01T10:00:00Z"),
            "",
            &[ana, me],
            "/v/a.md",
        )
        .unwrap();
        idx.save_speakers("a", &[speaker("speaker_1", Some("Ana"), vec![])])
            .unwrap();
        let mut who: Vec<String> = idx
            .search("deployment", 10)
            .unwrap()
            .into_iter()
            .map(|h| h.speaker)
            .collect();
        who.sort();
        assert_eq!(who, ["Ana", "Me"]);
    }
}
