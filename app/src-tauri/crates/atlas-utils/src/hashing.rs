//! Content hashing (§7.1: "hash, mtime, size" file metadata; §22: cache
//! invalidation keyed by content hash + parser/engine version).

use sha2::{Digest, Sha256};

/// Hash raw bytes to a hex-encoded SHA-256 digest. Used for source file
/// content hashes (§7.1) and cache invalidation keys (§22).
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_encode(&hasher.finalize())
}

/// Hash a UTF-8 string the same way as [`hash_bytes`].
pub fn hash_str(input: &str) -> String {
    hash_bytes(input.as_bytes())
}

/// Combine a content hash with a version tag into a single cache
/// invalidation key (§22: "Cache invalidation key: source file content hash
/// + parser/engine version tag").
pub fn cache_key(content_hash: &str, version_tag: &str) -> String {
    format!("{content_hash}:{version_tag}")
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_bytes_is_deterministic() {
        assert_eq!(hash_bytes(b"atlas"), hash_bytes(b"atlas"));
    }

    #[test]
    fn hash_bytes_differs_for_different_input() {
        assert_ne!(hash_bytes(b"atlas"), hash_bytes(b"atlas2"));
    }

    #[test]
    fn hash_bytes_produces_64_char_hex_string() {
        let digest = hash_bytes(b"atlas");
        assert_eq!(digest.len(), 64);
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_str_matches_hash_bytes() {
        assert_eq!(hash_str("atlas"), hash_bytes(b"atlas"));
    }

    #[test]
    fn cache_key_combines_hash_and_version() {
        assert_eq!(cache_key("abc123", "v1"), "abc123:v1");
    }
}
