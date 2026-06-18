//! Download + shasum verification.

#![allow(clippy::all)]

use sha1_smol::Sha1;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("http error: {0}")]
    Http(String),
    #[error("shasum mismatch: expected {expected}, got {actual}")]
    ShasumMismatch { expected: String, actual: String },
}

pub struct Downloader {
    client: reqwest::Client,
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new()
    }
}

impl Downloader {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("tusk/0.1.0 (+https://github.com/lschvn/tusk)")
                .pool_max_idle_per_host(64) // Bun uses 64 max-idle-conns-per-host
                .tcp_keepalive(std::time::Duration::from_secs(60))
                .build()
                .expect("reqwest client"),
        }
    }

    /// Download a URL and return the bytes. Does NOT verify shasum.
    pub async fn fetch(&self, url: &str) -> Result<Vec<u8>, DownloadError> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| DownloadError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(DownloadError::Http(format!(
                "HTTP {} for {}",
                resp.status(),
                url
            )));
        }

        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| DownloadError::Http(e.to_string()))
    }

    /// Download, verify shasum, return bytes on success.
    pub async fn fetch_and_verify(
        &self,
        url: &str,
        expected_shasum: &str,
    ) -> Result<Vec<u8>, DownloadError> {
        let bytes = self.fetch(url).await?;
        verify_shasum(&bytes, expected_shasum)?;
        Ok(bytes)
    }
}

/// Verify that the bytes match the expected sha1 shasum.
pub fn verify_shasum(bytes: &[u8], expected: &str) -> Result<(), DownloadError> {
    if expected.is_empty() {
        // No shasum to verify — accept (Composer allows empty shasum for some packages)
        return Ok(());
    }
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    let actual = hasher.digest().to_string();
    if actual == expected {
        Ok(())
    } else {
        Err(DownloadError::ShasumMismatch {
            expected: expected.to_string(),
            actual,
        })
    }
}
