use std::path::Path;

use serde::Serialize;

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FolderNode {
    pub name: String,
    /// Relative to the vault root, `/`-separated.
    pub path: String,
    pub children: Vec<FolderNode>,
}

/// Rejects absolute paths, `..`, hidden names and empty segments, so a folder can
/// never point outside the vault.
pub fn validate_folder(folder: &str) -> Result<(), StoreError> {
    if folder.is_empty() {
        return Ok(());
    }
    // Check the raw parts: `Path::components` silently drops `.` segments.
    let ok = !folder.starts_with('/')
        && folder.split('/').all(|part| {
            !part.trim().is_empty() && !part.starts_with('.') && !part.contains(['\\', ':', '\0'])
        });
    if ok {
        Ok(())
    } else {
        Err(StoreError::InvalidFolder(folder.to_string()))
    }
}

pub fn create_folder(vault: &Path, folder: &str) -> Result<(), StoreError> {
    if folder.trim().is_empty() {
        return Err(StoreError::InvalidFolder(folder.to_string()));
    }
    validate_folder(folder)?;
    std::fs::create_dir_all(vault.join(folder))?;
    Ok(())
}

/// The vault's folder tree (hidden folders skipped), sorted by name.
pub fn list_folders(vault: &Path) -> Result<Vec<FolderNode>, StoreError> {
    std::fs::create_dir_all(vault)?;
    walk(vault, "")
}

fn walk(dir: &Path, prefix: &str) -> Result<Vec<FolderNode>, StoreError> {
    let mut nodes = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let children = walk(&entry.path(), &path)?;
        nodes.push(FolderNode { name, path, children });
    }
    nodes.sort_by_key(|n| n.name.to_lowercase());
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_folder_paths() {
        for ok in ["", "Work", "Work/1-1s", "Clientes 2026"] {
            assert!(validate_folder(ok).is_ok(), "{ok}");
        }
        for bad in ["/etc", "../x", "Work/../../x", ".hidden", "Work/./x", "Work/ "] {
            assert!(validate_folder(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn lists_nested_folders_and_skips_hidden() {
        let vault = tempfile::tempdir().unwrap();
        create_folder(vault.path(), "Work/1-1s").unwrap();
        create_folder(vault.path(), "personal").unwrap();
        std::fs::create_dir(vault.path().join(".obsidian")).unwrap();
        std::fs::write(vault.path().join("note.md"), "x").unwrap();

        let tree = list_folders(vault.path()).unwrap();
        assert_eq!(
            tree.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
            ["personal", "Work"]
        );
        assert_eq!(tree[1].children[0].path, "Work/1-1s");
        assert!(create_folder(vault.path(), "  ").is_err());
    }
}
