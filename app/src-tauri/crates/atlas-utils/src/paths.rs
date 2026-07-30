//! Path helpers (§6: workspace root paths; §23: storage locations). No
//! path is ever hardcoded here -- callers always supply the root; these
//! helpers only do pure, reusable path arithmetic.

use std::path::{Path, PathBuf};

/// Join a root and a relative path, rejecting any relative path that would
/// escape the root via `..` (defense for §29: read-only, sandboxed access
/// to Source Documents).
pub fn safe_join(root: &Path, relative: &str) -> Option<PathBuf> {
    let relative_path = Path::new(relative);
    if relative_path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(root.join(relative_path))
}

/// Compute `path`'s location relative to `root`, as a forward-slash string
/// suitable for storing in `documents.relative_path` (§33.2).
pub fn relative_to(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
}

/// Extract a lowercase file extension (without the dot), or `None` if the
/// path has no extension. Used for file-type detection ahead of Parser
/// Selector resolution (§36.1: "extension + content sniffing").
pub fn extension_lower(path: &Path) -> Option<String> {
    path.extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_join_joins_normal_relative_path() {
        let root = Path::new("/workspace");
        assert_eq!(
            safe_join(root, "notes/chapter1.pdf").unwrap(),
            PathBuf::from("/workspace/notes/chapter1.pdf")
        );
    }

    #[test]
    fn safe_join_rejects_parent_traversal() {
        let root = Path::new("/workspace");
        assert!(safe_join(root, "../etc/passwd").is_none());
    }

    #[test]
    fn relative_to_strips_root_prefix() {
        let root = Path::new("/workspace");
        let full = Path::new("/workspace/notes/chapter1.pdf");
        assert_eq!(relative_to(root, full).unwrap(), "notes/chapter1.pdf");
    }

    #[test]
    fn relative_to_none_when_not_under_root() {
        let root = Path::new("/workspace");
        let full = Path::new("/other/chapter1.pdf");
        assert!(relative_to(root, full).is_none());
    }

    #[test]
    fn extension_lower_lowercases_extension() {
        assert_eq!(
            extension_lower(Path::new("Notes.PDF")).as_deref(),
            Some("pdf")
        );
    }

    #[test]
    fn extension_lower_none_when_missing() {
        assert!(extension_lower(Path::new("README")).is_none());
    }
}
