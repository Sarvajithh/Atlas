//! Filesystem helpers (§7.1: read-only access to Source Documents; §29:
//! "File access is strictly read-only for Source Documents"). These
//! helpers only read metadata/content; none of them write to, move, or
//! delete a source file.

use std::path::Path;

use crate::error::AppError;
use crate::hashing::hash_bytes;

/// Metadata captured for a source document (§7.1: "hash, mtime, size").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFingerprint {
    pub content_hash: String,
    pub size: u64,
    pub mtime_unix_secs: i64,
}

/// Read a file's bytes and compute its fingerprint (§7.1). Read-only --
/// never writes to, moves, or copies the source file (§6, §29).
pub fn fingerprint_file(path: &Path) -> Result<FileFingerprint, AppError> {
    let bytes = std::fs::read(path)?;
    let metadata = std::fs::metadata(path)?;
    let mtime_unix_secs = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    Ok(FileFingerprint {
        content_hash: hash_bytes(&bytes),
        size: bytes.len() as u64,
        mtime_unix_secs,
    })
}

/// Whether a path exists and is readable, without following it into a
/// write. Used to detect the "root folder missing/unreadable" workspace
/// error state (§45.1: Workspace Errors).
pub fn is_readable(path: &Path) -> bool {
    std::fs::File::open(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file_with(contents: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "atlas-utils-fs-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(contents).unwrap();
        path
    }

    #[test]
    fn fingerprint_file_reports_correct_size_and_hash() {
        let path = temp_file_with(b"hello atlas");
        let fp = fingerprint_file(&path).unwrap();
        assert_eq!(fp.size, 11);
        assert_eq!(fp.content_hash, hash_bytes(b"hello atlas"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fingerprint_file_missing_file_is_an_app_error() {
        let path = std::env::temp_dir().join("atlas-utils-fs-test-does-not-exist");
        assert!(fingerprint_file(&path).is_err());
    }

    #[test]
    fn is_readable_true_for_existing_file() {
        let path = temp_file_with(b"x");
        assert!(is_readable(&path));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn is_readable_false_for_missing_file() {
        let path = std::env::temp_dir().join("atlas-utils-fs-test-missing-xyz");
        assert!(!is_readable(&path));
    }
}
