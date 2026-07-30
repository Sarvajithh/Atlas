//! Initial recursive directory scan (§6.1 "Indexing (initial): full scan").
//! Pure filesystem traversal -- no parsing, no OCR, no chunking (§36.3):
//! this only discovers *which files exist* under a workspace root so they
//! can be turned into indexing jobs (§21, §36.1's "File change detected"
//! starting point, applied once at link time instead of via a live watch
//! event).

use std::path::{Path, PathBuf};

use atlas_utils::AppError;

/// Recursively list every regular file under `root`, skipping dotfiles/
/// dot-directories (editor swap files, `.git`, etc. -- not meaningful
/// study material) and symlinks (avoids accidental cycles; §29 read-only
/// access does not require following links).
pub fn scan_files(root: &Path) -> Result<Vec<PathBuf>, AppError> {
    let mut results = Vec::new();
    if !root.is_dir() {
        return Err(AppError::workspace(format!(
            "root path is not a readable directory: {}",
            root.display()
        )));
    }
    scan_into(root, &mut results)?;
    Ok(results)
}

fn scan_into(dir: &Path, results: &mut Vec<PathBuf>) -> Result<(), AppError> {
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }

        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        } else if file_type.is_dir() {
            scan_into(&path, results)?;
        } else if file_type.is_file() {
            results.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "atlas-watcher-scan-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scan_finds_nested_files() {
        let root = temp_dir("nested");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.pdf"), b"x").unwrap();
        std::fs::write(root.join("sub/b.pdf"), b"y").unwrap();

        let files = scan_files(&root).unwrap();
        assert_eq!(files.len(), 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_skips_dotfiles_and_dot_directories() {
        let root = temp_dir("dotfiles");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join(".git/config"), b"x").unwrap();
        std::fs::write(root.join(".DS_Store"), b"x").unwrap();
        std::fs::write(root.join("notes.md"), b"x").unwrap();

        let files = scan_files(&root).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name().unwrap(), "notes.md");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_missing_root_is_an_error() {
        let root = std::env::temp_dir().join("atlas-watcher-scan-does-not-exist-xyz");
        assert!(scan_files(&root).is_err());
    }
}
