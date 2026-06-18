//! Installer: orchestrates download → verify → cache → extract.

#![allow(clippy::all)]

use std::path::PathBuf;

use thiserror::Error;
use tusk_resolver::ResolvedDependency;

use crate::cache::Cache;
use crate::download::Downloader;
use crate::extract;

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("download error: {0}")]
    Download(#[from] crate::download::DownloadError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(String),
}

pub struct Installer {
    vendor_dir: PathBuf,
    cache: Cache,
    downloader: Downloader,
}

impl Installer {
    #[must_use]
    pub fn new(vendor_dir: PathBuf, cache_dir: PathBuf) -> Self {
        Self {
            vendor_dir,
            cache: Cache::new(cache_dir),
            downloader: Downloader::new(),
        }
    }

    /// Install all resolved dependencies in parallel.
    pub async fn install_all(&self, deps: &[ResolvedDependency]) -> Result<(), InstallError> {
        // Use futures to download in parallel
        let futures: Vec<_> = deps.iter().map(|d| self.install_one(d)).collect();
        let results = futures::future::join_all(futures).await;
        for result in results {
            result?;
        }
        Ok(())
    }

    /// Install a single package: download (or cache hit) → verify → extract.
    async fn install_one(&self, dep: &ResolvedDependency) -> Result<(), InstallError> {
        let shasum = &dep.dist.shasum;

        // Try cache first
        let archive_bytes = if self.cache.has(shasum) {
            // Cache hit — read from disk
            self.cache.read(shasum).ok_or_else(|| {
                InstallError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("cache miss for {shasum}"),
                ))
            })?
        } else {
            // Cache miss — download and verify
            let bytes = self
                .downloader
                .fetch_and_verify(&dep.dist.url, shasum)
                .await?;

            // Store in cache for next time
            let _ = self.cache.store(shasum, &bytes);

            bytes
        };

        // Extract to vendor/{vendor}/{package}/
        let pkg_dir = self.vendor_dir.join(&dep.name);

        // Atomic extract: write to temp dir, then rename
        let temp_dir = pkg_dir.with_extension("tmp_install");
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir)?;
        }

        extract::extract_zip(&archive_bytes, &temp_dir)
            .map_err(|e| InstallError::Zip(e.to_string()))?;

        // Atomic rename
        if pkg_dir.exists() {
            std::fs::remove_dir_all(&pkg_dir)?;
        }
        std::fs::rename(&temp_dir, &pkg_dir)?;

        Ok(())
    }
}
