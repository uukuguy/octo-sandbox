//! BlobStore — content-addressed external storage for large tool outputs.
//!
//! When a tool result exceeds `BLOB_THRESHOLD_BYTES`, the harness stores the
//! full content in the blob store and replaces the message content with a
//! compact reference `[blob:sha256:<hash>]`. On session reload, these references
//! save context tokens while the full content remains retrievable.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use tracing::debug;

/// Blob reference prefix/suffix markers.
pub const BLOB_PREFIX: &str = "[blob:sha256:";
pub const BLOB_SUFFIX: &str = "]";

/// Tool outputs larger than this threshold are automatically externalized.
pub const BLOB_THRESHOLD_BYTES: usize = 4096;

/// Content-addressed blob store using SHA-256 hashing.
///
/// Storage layout: `base_dir/<hash[0..2]>/<hash[2..]>`
/// Two-level directory structure prevents single-directory overload.
pub struct BlobStore {
    base_dir: PathBuf,
}

impl BlobStore {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Store content, returning the SHA-256 hex hash.
    pub fn store(&self, content: &[u8]) -> Result<String> {
        let hash = Self::hash(content);
        let path = self.blob_path(&hash);

        if path.exists() {
            debug!(hash = %hash, "Blob already exists, skipping write");
            return Ok(hash);
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create blob dir: {}", parent.display()))?;
        }

        fs::write(&path, content)
            .with_context(|| format!("Failed to write blob: {}", path.display()))?;

        debug!(hash = %hash, size = content.len(), "Blob stored");
        Ok(hash)
    }

    /// Load content by hash.
    pub fn load(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.blob_path(hash);
        fs::read(&path)
            .with_context(|| format!("Failed to read blob {}: {}", hash, path.display()))
    }

    /// Check if a blob exists.
    pub fn exists(&self, hash: &str) -> bool {
        self.blob_path(hash).exists()
    }

    /// Parse a blob reference string, returning the hash if valid.
    ///
    /// Input: `"[blob:sha256:abcdef...]"` → `Some("abcdef...")`
    pub fn parse_blob_ref(text: &str) -> Option<&str> {
        let trimmed = text.trim();
        if trimmed.starts_with(BLOB_PREFIX) && trimmed.ends_with(BLOB_SUFFIX) {
            let hash = &trimmed[BLOB_PREFIX.len()..trimmed.len() - BLOB_SUFFIX.len()];
            if !hash.is_empty() && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(hash);
            }
        }
        None
    }

    /// Format a blob reference from a hash.
    pub fn format_blob_ref(hash: &str) -> String {
        format!("{BLOB_PREFIX}{hash}{BLOB_SUFFIX}")
    }

    /// Resolve a blob reference: if text is a blob ref, load and return content.
    /// Otherwise return the text as-is.
    pub fn resolve(&self, text: &str) -> Result<String> {
        if let Some(hash) = Self::parse_blob_ref(text) {
            let content = self.load(hash)?;
            String::from_utf8(content).with_context(|| "Blob content is not valid UTF-8")
        } else {
            Ok(text.to_string())
        }
    }

    /// Compute SHA-256 hash of content.
    fn hash(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        format!("{:x}", hasher.finalize())
    }

    /// Compute the filesystem path for a given hash.
    fn blob_path(&self, hash: &str) -> PathBuf {
        if hash.len() < 2 {
            return self.base_dir.join(hash);
        }
        self.base_dir.join(&hash[..2]).join(&hash[2..])
    }

    /// Get the base directory of this store.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_store() -> (BlobStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = BlobStore::new(dir.path().to_path_buf());
        (store, dir)
    }

    #[test]
    fn test_store_and_load_roundtrip() {
        let (store, _dir) = test_store();
        let content = b"Hello, blob world!";
        let hash = store.store(content).unwrap();

        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));

        let loaded = store.load(&hash).unwrap();
        assert_eq!(loaded, content);
    }

    #[test]
    fn test_sha256_correctness() {
        // Known SHA-256 for empty string
        let hash = BlobStore::hash(b"");
        assert_eq!(hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn test_two_level_directory_structure() {
        let (store, dir) = test_store();
        let hash = store.store(b"test content").unwrap();

        // Should create <dir>/<first-2-chars>/<remaining> structure
        let prefix = &hash[..2];
        let suffix = &hash[2..];
        let expected = dir.path().join(prefix).join(suffix);
        assert!(expected.exists(), "Expected two-level path: {}", expected.display());
    }

    #[test]
    fn test_parse_blob_ref_valid() {
        let hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let ref_str = BlobStore::format_blob_ref(hash);
        assert_eq!(ref_str, "[blob:sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855]");

        let parsed = BlobStore::parse_blob_ref(&ref_str);
        assert_eq!(parsed, Some(hash));
    }

    #[test]
    fn test_parse_blob_ref_invalid() {
        assert!(BlobStore::parse_blob_ref("not a blob ref").is_none());
        assert!(BlobStore::parse_blob_ref("[blob:sha256:]").is_none());
        assert!(BlobStore::parse_blob_ref("[blob:sha256:short]").is_none());
        assert!(BlobStore::parse_blob_ref("").is_none());
    }

    #[test]
    fn test_threshold_constant() {
        assert_eq!(BLOB_THRESHOLD_BYTES, 4096);
    }

    #[test]
    fn test_exists() {
        let (store, _dir) = test_store();
        let hash = store.store(b"exists test").unwrap();
        assert!(store.exists(&hash));
        assert!(!store.exists("0000000000000000000000000000000000000000000000000000000000000000"));
    }

    #[test]
    fn test_dedup_write() {
        let (store, _dir) = test_store();
        let content = b"duplicate content";
        let hash1 = store.store(content).unwrap();
        let hash2 = store.store(content).unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_resolve_blob_ref() {
        let (store, _dir) = test_store();
        let content = "large tool output here";
        let hash = store.store(content.as_bytes()).unwrap();
        let ref_str = BlobStore::format_blob_ref(&hash);

        let resolved = store.resolve(&ref_str).unwrap();
        assert_eq!(resolved, content);
    }

    #[test]
    fn test_resolve_plain_text() {
        let (store, _dir) = test_store();
        let text = "just plain text, not a blob ref";
        let resolved = store.resolve(text).unwrap();
        assert_eq!(resolved, text);
    }
}
