//! Content-addressed cache for downloaded archives.
//!
//! Archive stored at `{cache_root}/{shasum}/archive.zip`.
//! If the dir exists, the archive is already cached.

#![allow(clippy::all)]

use std::path::PathBuf;

pub struct Cache {
    pub root: PathBuf,
}

impl Cache {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Returns the path where this archive would be cached.
    /// Does NOT check existence — use `has()`.
    #[must_use]
    pub fn archive_path(&self, shasum: &str) -> PathBuf {
        self.root.join(shasum).join("archive.zip")
    }

    /// Returns true if the archive is already in the cache.
    pub fn has(&self, shasum: &str) -> bool {
        self.archive_path(shasum).exists()
    }

    /// Store bytes in the cache keyed by shasum.
    pub fn store(&self, shasum: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
        let dir = self.root.join(shasum);
        std::fs::create_dir_all(&dir)?;
        let archive_path = dir.join("archive.zip");
        std::fs::write(&archive_path, bytes)?;
        Ok(archive_path)
    }

    /// Read cached archive bytes, or None if not cached.
    pub fn read(&self, shasum: &str) -> Option<Vec<u8>> {
        if self.has(shasum) {
            std::fs::read(self.archive_path(shasum)).ok()
        } else {
            None
        }
    }
}
