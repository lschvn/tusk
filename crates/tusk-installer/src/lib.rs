//! `tusk-installer` — download, verify, extract.
//!
//! Per GOAL.md §7.5, this is where the headline speed comes from:
//!   - parallel HTTP via `reqwest` + `tokio`
//!   - content-addressed cache so repeat installs skip the network
//!   - atomic extract so partial failures never leave junk in `vendor/`
//!   - shasum verification (rejects tampered archives)

#![forbid(unsafe_code)]

mod cache;
mod curl_downloader;
mod download;
mod extract;
mod installer;

pub use cache::Cache;
pub use curl_downloader::{download_batch, CurlError};
pub use download::{verify_shasum, DownloadError, Downloader};
pub use installer::{InstallError, Installer};
