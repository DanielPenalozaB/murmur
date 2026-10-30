//! Renaming, moving and deleting sessions and folders. The vault files move first and
//! the index follows, so a failure never leaves the index pointing at a missing file.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use murmur_core::{ExportFormat, Session};

use crate::{Index, StoreError, file_name, validate_folder};

/// Sets the title and folder in a session file's header. Everything else, including
/// a heading the user removed, stays as it is.
pub fn replace_header(text: &str, format: ExportFormat, title: &str, folder: &str) -> String {
    match format {
        ExportFormat::Markdown => {
            let Some(body) = text.strip_prefix("---\n") else {
                return text.to_string();
            };
            let Some(end) = body.find("\n---\n") else {
                return text.to_string();
            };
            let front: Vec<String> = body[..end]
                .lines()
                .map(|line| {
                    if line.starts_with("title: ") {
                        format!("title: {}", yaml_string(title))
                    } else if line.starts_with("folder: ") {
                        format!("folder: {}", yaml_string(folder))
                    } else {
                        line.to_string()
                    }
                })
                .collect();
            let rest = &body[end + "\n---\n".len()..];
            // The `# Title` heading that follows the frontmatter, if it's still there.
            let rest = match rest.trim_start_matches('\n').strip_prefix("# ") {
                Some(after) => {
                    let blank = rest.len() - rest.trim_start_matches('\n').len();
                    let line_end = after.find('\n').unwrap_or(after.len());
                    format!("{}# {title}{}", &rest[..blank], &after[line_end..])
                }
                None => rest.to_string(),
            };
            format!("---\n{}\n---\n{rest}", front.join("\n"))
        }
        ExportFormat::Txt => match text.split_once('\n') {
            Some((_, rest)) => format!("{title}\n{rest}"),
            None => title.to_string(),
        },
    }
}

fn yaml_string(s: &str) -> String {
    serde_json::to_string(s).expect("strings always serialize")
}

/// `name` in `dir`, or `name 2`, `name 3`… if taken.
fn free_path(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    if !path.exists() {
        return path;
    }
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    (2..)
        .map(|n| dir.join(format!("{stem} {n}.{ext}")))
        .find(|p| !p.exists())
        .expect("some number is free")
}

/// Moves the session file to where `session`'s folder and title put it, with the header
/// updated, never overwriting another file. Returns the new path.
fn relocate_file(vault: &Path, current: &Path, session: &Session) -> Result<PathBuf, StoreError> {
    validate_folder(&session.folder)?;
    let text = std::fs::read_to_string(current)?;
    let dir = vault.join(&session.folder);
    std::fs::create_dir_all(&dir)?;
    let name = file_name(session);
    let target = if current.parent() == Some(dir.as_path())
        && current.file_name().and_then(|n| n.to_str()) == Some(name.as_str())
    {
        current.to_path_buf()
    } else {
        free_path(&dir, &name)
    };
    let text = replace_header(&text, session.format, &session.title, &session.folder);
    // Write the new file before removing the old one, so a failure loses nothing.
    std::fs::write(&target, text)?;
    if target != current {
        std::fs::remove_file(current)?;
    }
    Ok(target)
}

/// Renames a session and/or moves it to another folder, file and index together.
pub fn relocate_session(
    index: &Index,
    vault: &Path,
    id: &str,
    title: &str,
    folder: &str,
) -> Result<(), StoreError> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 200 || title.contains(['\n', '\r']) {
        return Err(StoreError::InvalidTitle);
    }
    let (record, _) = index
        .get_session(id)?
        .ok_or_else(|| StoreError::NotFound(format!("session {id}")))?;
    let session = Session {
        title: title.to_string(),
        folder: folder.to_string(),
        ..record.session
    };
    let path = relocate_file(vault, Path::new(&record.file_path), &session)?;
    index.relocate_session(id, &session.title, &session.folder, &path.to_string_lossy())
}

/// Renames or moves a folder, with its subfolders and sessions. Fails if `to` exists.
pub fn move_folder(index: &Index, vault: &Path, from: &str, to: &str) -> Result<(), StoreError> {
    validate_folder(from)?;
    validate_folder(to)?;
    if from.is_empty() || to.is_empty() {
        return Err(StoreError::InvalidFolder(String::new()));
    }
    if to == from || to.starts_with(&format!("{from}/")) {
        return Err(StoreError::InvalidFolder(to.to_string()));
    }
    let target = vault.join(to);
    if target.exists() {
        return Err(StoreError::FolderExists(to.to_string()));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(vault.join(from), &target)?;

    let old_root = vault.join(from);
    for record in index.sessions_under(from)? {
        let folder = format!("{to}{}", &record.session.folder[from.len()..]);
        let old = Path::new(&record.file_path);
        let path = match old.strip_prefix(&old_root) {
            Ok(rest) => target.join(rest),
            // Files kept outside the folder (moved by hand) stay where they are.
            Err(_) => old.to_path_buf(),
        };
        if let Ok(text) = std::fs::read_to_string(&path) {
            let format = record.session.format;
            std::fs::write(
                &path,
                replace_header(&text, format, &record.session.title, &folder),
            )?;
        }
        index.relocate_session(
            &record.session.id,
            &record.session.title,
            &folder,
            &path.to_string_lossy(),
        )?;
    }
    Ok(())
}

/// Deletes a folder that holds no sessions and no files of the user's (Finder's
/// `.DS_Store` doesn't count). Empty subfolders go with it.
pub fn delete_folder(index: &Index, vault: &Path, folder: &str) -> Result<(), StoreError> {
    validate_folder(folder)?;
    if folder.is_empty() {
        return Err(StoreError::InvalidFolder(String::new()));
    }
    if !index.sessions_under(folder)?.is_empty() {
        return Err(StoreError::FolderNotEmpty(folder.to_string()));
    }
    let dir = vault.join(folder);
    if has_files(&dir)? {
        return Err(StoreError::FolderNotEmpty(folder.to_string()));
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn has_files(dir: &Path) -> Result<bool, StoreError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if has_files(&entry.path())? {
                return Ok(true);
            }
        } else if entry.file_name() != ".DS_Store" {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Points the index at session files moved or renamed outside Murmur (in Finder,
/// Obsidian…). Markdown files are recognized by the session id in their frontmatter,
/// plain text by their file name. Returns how many sessions were found elsewhere.
pub fn sync_vault(index: &Index, vault: &Path) -> Result<usize, StoreError> {
    let sessions = index.list_sessions(None)?;
    let (present, missing): (Vec<_>, Vec<_>) = sessions
        .into_iter()
        .partition(|r| Path::new(&r.file_path).exists());
    if missing.is_empty() || !vault.is_dir() {
        return Ok(0);
    }
    let claimed: HashSet<PathBuf> = present.iter().map(|r| PathBuf::from(&r.file_path)).collect();
    let mut files = Vec::new();
    collect_session_files(vault, &mut files)?;
    files.retain(|f| !claimed.contains(f));

    let mut found = 0;
    for record in missing {
        let id = &record.session.id;
        let by_id = files
            .iter()
            .find(|f| frontmatter_session(f).as_deref() == Some(id.as_str()));
        let name = Path::new(&record.file_path).file_name();
        let mut by_name = files.iter().filter(|f| f.file_name() == name);
        let unique_name = match (by_name.next(), by_name.next()) {
            (Some(f), None) => Some(f),
            _ => None,
        };
        let Some(path) = by_id.or(unique_name).cloned() else {
            continue;
        };
        let Some(folder) = folder_of(vault, &path) else {
            continue;
        };
        index.relocate_session(id, &record.session.title, &folder, &path.to_string_lossy())?;
        files.retain(|f| *f != path);
        found += 1;
    }
    Ok(found)
}

/// `.md` and `.txt` files in the vault, hidden folders skipped.
fn collect_session_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), StoreError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if entry.file_type()?.is_dir() {
            collect_session_files(&path, out)?;
        } else if matches!(path.extension().and_then(|e| e.to_str()), Some("md" | "txt")) {
            out.push(path);
        }
    }
    Ok(())
}

/// The `session:` id in a Markdown file's frontmatter.
fn frontmatter_session(path: &Path) -> Option<String> {
    if path.extension()? != "md" {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let front = text.strip_prefix("---\n")?;
    let front = &front[..front.find("\n---")?];
    front
        .lines()
        .find_map(|line| line.strip_prefix("session: "))
        .map(|id| id.trim().trim_matches('"').to_string())
}

/// The vault folder a file sits in, `/`-separated; `None` outside the vault.
fn folder_of(vault: &Path, file: &Path) -> Option<String> {
    let dir = file.parent()?.strip_prefix(vault).ok()?;
    let parts: Option<Vec<&str>> = dir.components().map(|c| c.as_os_str().to_str()).collect();
    let folder = parts?.join("/");
    validate_folder(&folder).ok()?;
    Some(folder)
}

/// Removes a session from the vault and the index. A file already gone is fine.
pub fn delete_session(index: &Index, id: &str) -> Result<(), StoreError> {
    let (record, _) = index
        .get_session(id)?
        .ok_or_else(|| StoreError::NotFound(format!("session {id}")))?;
    match std::fs::remove_file(&record.file_path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    index.delete_session(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write_session_file;
    use murmur_core::SpeakerNames;

    fn session(id: &str, title: &str, folder: &str, format: ExportFormat) -> Session {
        Session {
            id: id.into(),
            title: title.into(),
            folder: folder.into(),
            started_at: "2026-10-12T10:00:00-05:00".into(),
            duration_ms: 60_000,
            format,
        }
    }

    /// A vault with the session written to it and indexed.
    fn add(index: &mut Index, vault: &Path, s: &Session) -> PathBuf {
        let path = write_session_file(vault, s, "- a note", &[], &SpeakerNames::new()).unwrap();
        index
            .save_session(s, "- a note", &[], &path.to_string_lossy())
            .unwrap();
        path
    }

    fn record(index: &Index, id: &str) -> (Session, String) {
        let (r, _) = index.get_session(id).unwrap().unwrap();
        (r.session, std::fs::read_to_string(&r.file_path).unwrap())
    }

    #[test]
    fn replaces_markdown_header_only() {
        let text = "---\ntitle: \"Old\"\ndate: x\nfolder: \"Work\"\nsession: a\n---\n\n# Old\n\nOld stays in the body.\n";
        assert_eq!(
            replace_header(text, ExportFormat::Markdown, "New: \"Q4\"", "Work/1-1s"),
            "---\ntitle: \"New: \\\"Q4\\\"\"\ndate: x\nfolder: \"Work/1-1s\"\nsession: a\n---\n\n# New: \"Q4\"\n\nOld stays in the body.\n"
        );
        let no_heading = "---\ntitle: \"Old\"\n---\n\nText\n";
        assert_eq!(
            replace_header(no_heading, ExportFormat::Markdown, "New", ""),
            "---\ntitle: \"New\"\n---\n\nText\n"
        );
        assert_eq!(
            replace_header("Edited by hand", ExportFormat::Markdown, "New", ""),
            "Edited by hand"
        );
        assert_eq!(
            replace_header("Old\nx · 1:00\n", ExportFormat::Txt, "New", ""),
            "New\nx · 1:00\n"
        );
    }

    #[test]
    fn renames_and_moves_sessions() {
        let vault = tempfile::tempdir().unwrap();
        let mut index = Index::open_in_memory().unwrap();
        let old = add(
            &mut index,
            vault.path(),
            &session("a", "Standup", "Work", ExportFormat::Markdown),
        );
        // Another file already has the new name.
        add(
            &mut index,
            vault.path(),
            &session("b", "Retro", "Team", ExportFormat::Markdown),
        );

        relocate_session(&index, vault.path(), "a", " Retro ", "Team").unwrap();
        let (s, text) = record(&index, "a");
        assert_eq!((s.title.as_str(), s.folder.as_str()), ("Retro", "Team"));
        assert!(text.contains("title: \"Retro\"\n") && text.contains("folder: \"Team\"\n"));
        assert!(text.contains("# Retro\n") && text.contains("- a note"));
        assert!(!old.exists());
        assert!(vault.path().join("Team/2026-10-12 Retro 2.md").exists());
        assert_eq!(index.list_sessions(Some("Team")).unwrap().len(), 2);

        // Same name and folder: rewritten in place, not renamed to "… 2".
        let (r, _) = index.get_session("b").unwrap().unwrap();
        relocate_session(&index, vault.path(), "b", "Retro", "Team").unwrap();
        assert_eq!(index.get_session("b").unwrap().unwrap().0.file_path, r.file_path);

        for bad in ["", "  ", "Two\nlines"] {
            assert!(
                relocate_session(&index, vault.path(), "a", bad, "Team").is_err(),
                "{bad:?}"
            );
        }
        assert!(relocate_session(&index, vault.path(), "a", "X", "../out").is_err());
        assert!(relocate_session(&index, vault.path(), "zz", "X", "").is_err());
    }

    #[test]
    fn moves_folders_with_their_sessions() {
        let vault = tempfile::tempdir().unwrap();
        let mut index = Index::open_in_memory().unwrap();
        add(
            &mut index,
            vault.path(),
            &session("a", "One", "Work", ExportFormat::Markdown),
        );
        add(
            &mut index,
            vault.path(),
            &session("b", "Two", "Work/1-1s", ExportFormat::Txt),
        );
        add(
            &mut index,
            vault.path(),
            &session("c", "Three", "Workshop", ExportFormat::Markdown),
        );

        move_folder(&index, vault.path(), "Work", "Archive/Work 2025").unwrap();
        let (a, text) = record(&index, "a");
        assert_eq!(a.folder, "Archive/Work 2025");
        assert!(text.contains("folder: \"Archive/Work 2025\""));
        let (b, _) = record(&index, "b");
        assert_eq!(b.folder, "Archive/Work 2025/1-1s");
        assert!(
            vault
                .path()
                .join("Archive/Work 2025/1-1s/2026-10-12 Two.txt")
                .exists()
        );
        assert_eq!(
            record(&index, "c").0.folder,
            "Workshop",
            "only whole folder names match"
        );
        assert!(!vault.path().join("Work").exists());

        assert!(matches!(
            move_folder(&index, vault.path(), "Workshop", "Archive/Work 2025"),
            Err(StoreError::FolderExists(_))
        ));
        assert!(move_folder(&index, vault.path(), "Archive", "Archive/inside").is_err());
        assert!(move_folder(&index, vault.path(), "", "x").is_err());
    }

    #[test]
    fn deletes_only_empty_folders() {
        let vault = tempfile::tempdir().unwrap();
        let mut index = Index::open_in_memory().unwrap();
        add(
            &mut index,
            vault.path(),
            &session("a", "One", "Work/1-1s", ExportFormat::Markdown),
        );
        crate::create_folder(vault.path(), "Old/empty").unwrap();
        std::fs::write(vault.path().join("Old/.DS_Store"), "").unwrap();
        crate::create_folder(vault.path(), "Mine").unwrap();
        std::fs::write(vault.path().join("Mine/todo.md"), "x").unwrap();

        assert!(matches!(
            delete_folder(&index, vault.path(), "Work"),
            Err(StoreError::FolderNotEmpty(_))
        ));
        assert!(matches!(
            delete_folder(&index, vault.path(), "Mine"),
            Err(StoreError::FolderNotEmpty(_))
        ));
        delete_folder(&index, vault.path(), "Old").unwrap();
        assert!(!vault.path().join("Old").exists());
        assert!(delete_folder(&index, vault.path(), "").is_err());
    }

    #[test]
    fn finds_files_moved_outside_murmur() {
        let vault = tempfile::tempdir().unwrap();
        let mut index = Index::open_in_memory().unwrap();
        add(
            &mut index,
            vault.path(),
            &session("a", "Standup", "Work", ExportFormat::Markdown),
        );
        add(
            &mut index,
            vault.path(),
            &session("b", "Notes", "Work", ExportFormat::Txt),
        );
        add(
            &mut index,
            vault.path(),
            &session("c", "Gone", "Work", ExportFormat::Markdown),
        );
        assert_eq!(sync_vault(&index, vault.path()).unwrap(), 0);

        // The folder renamed in Finder, the Markdown file renamed too, the third deleted.
        let v = vault.path();
        std::fs::rename(v.join("Work"), v.join("Team")).unwrap();
        std::fs::rename(v.join("Team/2026-10-12 Standup.md"), v.join("Team/Daily.md")).unwrap();
        std::fs::remove_file(v.join("Team/2026-10-12 Gone.md")).unwrap();

        assert_eq!(sync_vault(&index, v).unwrap(), 2);
        let (a, _) = index.get_session("a").unwrap().unwrap();
        assert_eq!(a.session.folder, "Team");
        assert_eq!(Path::new(&a.file_path), v.join("Team/Daily.md"));
        assert_eq!(record(&index, "b").0.folder, "Team");
        assert_eq!(index.get_session("c").unwrap().unwrap().0.session.folder, "Work");
        assert_eq!(sync_vault(&index, v).unwrap(), 0);
    }

    #[test]
    fn deletes_sessions() {
        let vault = tempfile::tempdir().unwrap();
        let mut index = Index::open_in_memory().unwrap();
        let path = add(
            &mut index,
            vault.path(),
            &session("a", "One", "", ExportFormat::Markdown),
        );
        add(
            &mut index,
            vault.path(),
            &session("b", "Two", "", ExportFormat::Markdown),
        );
        delete_session(&index, "a").unwrap();
        assert!(!path.exists());
        assert!(index.get_session("a").unwrap().is_none());

        // The file was already deleted in Finder.
        let (b, _) = index.get_session("b").unwrap().unwrap();
        std::fs::remove_file(&b.file_path).unwrap();
        delete_session(&index, "b").unwrap();
        assert!(index.list_sessions(None).unwrap().is_empty());
    }
}
