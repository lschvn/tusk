//! Installer: orchestrates batch download → verify → cache → extract.
//!
//! Uses libcurl's multi interface for HTTP/2 multiplexed batch downloads.
//! All archives from the same host (codeload.github.com) download over a
//! single multiplexed HTTP/2 connection — no per-request TLS handshake.

#![allow(clippy::all)]

use std::path::PathBuf;

use thiserror::Error;
use tusk_resolver::ResolvedDependency;

use crate::cache::Cache;
use crate::curl_downloader;
use crate::download::verify_shasum;
use crate::extract;

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("download error: {0}")]
    Download(String),
    #[error("shasum mismatch: expected {expected}, got {actual}")]
    ShasumMismatch { expected: String, actual: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(String),
}

pub struct Installer {
    vendor_dir: PathBuf,
    cache: Cache,
}

impl Installer {
    #[must_use]
    pub fn new(vendor_dir: PathBuf, cache_dir: PathBuf) -> Self {
        Self {
            vendor_dir,
            cache: Cache::new(cache_dir),
        }
    }

    /// Install all resolved dependencies.
    ///
    /// Two-phase approach:
    /// 1. **Batch download** all cache misses in a single libcurl multi call
    ///    (HTTP/2 multiplexing → one TLS connection per host).
    /// 2. **Parallel extract** all archives using `spawn_blocking`.
    pub async fn install_all(&self, deps: &[ResolvedDependency]) -> Result<(), InstallError> {
        if deps.is_empty() {
            return Ok(());
        }

        // Phase 1: Determine what we need to download
        let mut to_download: Vec<(usize, String)> = Vec::new(); // (dep_index, url)
        let mut archive_bytes: Vec<Option<Vec<u8>>> = vec![None; deps.len()];

        for (i, dep) in deps.iter().enumerate() {
            let shasum = &dep.dist.shasum;
            if self.cache.has(shasum) {
                // Cache hit
                if let Some(bytes) = self.cache.read(shasum) {
                    archive_bytes[i] = Some(bytes);
                    continue;
                }
            }
            // Cache miss — need to download
            to_download.push((i, dep.dist.url.clone()));
        }

        // Phase 2: Batch download all cache misses
        if !to_download.is_empty() {
            let urls: Vec<String> = to_download.iter().map(|(_, url)| url.clone()).collect();

            // Run libcurl multi on a blocking thread (it uses synchronous I/O)
            let results = tokio::task::spawn_blocking(move || {
                curl_downloader::download_batch(&urls)
            })
            .await
            .map_err(|e| InstallError::Download(format!("thread pool error: {e}")))?
            .map_err(|e| InstallError::Download(e.to_string()))?;

            // Process results: verify shasum + cache
            for ((dep_idx, _), result) in to_download.into_iter().zip(results.into_iter()) {
                match result {
                    Ok(bytes) => {
                        // Verify shasum
                        let dep = &deps[dep_idx];
                        verify_shasum(&bytes, &dep.dist.shasum)
                            .map_err(|e| InstallError::Download(e.to_string()))?;

                        // Store in cache
                        let _ = self.cache.store(&dep.dist.shasum, &bytes);

                        archive_bytes[dep_idx] = Some(bytes);
                    }
                    Err(e) => {
                        return Err(InstallError::Download(format!(
                            "failed to download {}: {}",
                            deps[dep_idx].name, e
                        )));
                    }
                }
            }
        }

        // Phase 3: Extract all archives in parallel using spawn_blocking
        let extract_tasks: Vec<_> = archive_bytes
            .iter()
            .enumerate()
            .filter_map(|(i, bytes)| {
                bytes.as_ref().map(|b| {
                    let dep = &deps[i];
                    let pkg_dir = self.vendor_dir.join(&dep.name);
                    let bytes = b.clone();
                    tokio::task::spawn_blocking(move || {
                        let temp_dir = pkg_dir.with_extension("tmp_install");
                        if temp_dir.exists() {
                            std::fs::remove_dir_all(&temp_dir)?;
                        }
                        extract::extract_zip(&bytes, &temp_dir)?;
                        // Atomic rename
                        if pkg_dir.exists() {
                            std::fs::remove_dir_all(&pkg_dir)?;
                        }
                        std::fs::rename(&temp_dir, &pkg_dir)?;
                        Ok::<_, std::io::Error>(())
                    })
                })
            })
            .collect();

        // Wait for all extractions
        for task in extract_tasks {
            task.await
                .map_err(|e| InstallError::Zip(format!("join error: {e}")))?
                .map_err(InstallError::Io)?;
        }

        Ok(())
    }
}
