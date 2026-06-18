//! Stub: filled in at Step 5 (TDD).
#![allow(dead_code, clippy::all)]

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

impl Downloader {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}
