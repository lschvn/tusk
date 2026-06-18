//! Packagist p2 client.
//!
//! Fetches and parses `https://repo.packagist.org/p2/{vendor}/{package}.json`.
//! The response shape is:
//! ```json
//! { "packages": { "vendor/pkg": [ { "version": "1.0.0", "dist": {...}, "require": {...} }, ... ] } }
//! ```
#![allow(clippy::all)]

use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashMap;
use thiserror::Error;
use tusk_manifest::RequireMap;
use tusk_semver::Version;

/// Error type for all registry operations.
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("network error: {0}")]
    Network(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("package not found: {0}")]
    NotFound(String),
}

/// Trait abstraction so resolver/installer tests can run fully offline against
/// a mock — no real network.
#[async_trait]
pub trait Registry: Send + Sync {
    /// Fetch all version metadata for a package. Implementations should cache.
    async fn package_metadata(
        &self,
        vendor: &str,
        package: &str,
    ) -> Result<PackageMetadata, RegistryError>;
}

/// All known versions of a single package.
#[derive(Debug, Clone)]
pub struct PackageMetadata {
    pub versions: Vec<PackageVersion>,
}

/// One version's metadata.
#[derive(Debug, Clone)]
pub struct PackageVersion {
    pub version: Version,
    pub dist: DistRef,
    pub require: RequireMap,
}

/// Download reference for a package version (dist = prebuilt archive).
#[derive(Debug, Clone)]
pub struct DistRef {
    pub url: String,
    pub shasum: String,
    pub r#type: String,
}

// ---------------------------------------------------------------------------
// PackagistClient — real HTTP implementation
// ---------------------------------------------------------------------------

/// Real Packagist p2 API client with in-process metadata caching.
#[derive(Clone)]
pub struct PackagistClient {
    base_url: String,
    http: reqwest::Client,
    /// In-process cache so a second fetch for the same package within a run
    /// does not hit the network. Key = "vendor/package".
    cache: std::sync::Arc<RwLock<HashMap<String, PackageMetadata>>>,
}

impl PackagistClient {
    /// Create a client pointing at the given base URL (no trailing slash).
    /// `PackagistClient::new("https://repo.packagist.org")` is the default.
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        let mut base = base_url.into();
        // Normalize: strip trailing slash so `/p2/...` is appended cleanly.
        while base.ends_with('/') {
            base.pop();
        }
        // Set a User-Agent so download endpoints that require one
        // (e.g. codeload.github.com) don't return 403 Forbidden.
        let http = reqwest::Client::builder()
            .user_agent("tusk/0.1.0 (+https://github.com/lschvn/tusk)")
            .pool_max_idle_per_host(64)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .build()
            .expect("reqwest client");
        Self {
            base_url: base,
            http,
            cache: std::sync::Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl Registry for PackagistClient {
    async fn package_metadata(
        &self,
        vendor: &str,
        package: &str,
    ) -> Result<PackageMetadata, RegistryError> {
        let key = format!("{vendor}/{package}");

        // Cache hit?
        {
            let cache = self.cache.read();
            if let Some(meta) = cache.get(&key) {
                return Ok(meta.clone());
            }
        }

        // Cache miss → fetch.
        let url = format!("{}/p2/{key}.json", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| RegistryError::Network(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::NotFound(key));
        }
        if !resp.status().is_success() {
            return Err(RegistryError::Network(format!(
                "HTTP {} for {}",
                resp.status(),
                url
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| RegistryError::Network(e.to_string()))?;

        let meta = parse_p2_response(&key, &body)?;

        // Store in cache.
        {
            let mut cache = self.cache.write();
            cache.insert(key, meta.clone());
        }

        Ok(meta)
    }
}

/// Parse a Packagist p2 JSON response into `PackageMetadata`.
fn parse_p2_response(
    key: &str,
    body: &serde_json::Value,
) -> Result<PackageMetadata, RegistryError> {
    let versions_arr = body
        .get("packages")
        .and_then(|p| p.get(key))
        .and_then(|v| v.as_array())
        .ok_or_else(|| RegistryError::Parse(format!("missing packages.{key} array in response")))?;

    let mut versions = Vec::with_capacity(versions_arr.len());
    for entry in versions_arr {
        let version_str = entry
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RegistryError::Parse("missing version field".to_string()))?;

        // Skip versions we can't parse (dev branches with custom stability suffixes,
        // weird version strings, etc.). These are typically source-only installs.
        let Ok(version) = Version::parse(version_str) else {
            continue;
        };

        // Skip versions without a dist field (dev branches, source-only entries).
        // These are git-source installs, which are out of scope for Phase 1 (dist-only).
        let Some(dist_obj) = entry.get("dist") else {
            continue;
        };

        // Also skip if dist.url is empty (defensive — should not happen with a dist object)
        let dist_url = dist_obj.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if dist_url.is_empty() {
            continue;
        }

        let dist = DistRef {
            url: dist_url.to_string(),
            shasum: dist_obj
                .get("shasum")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            r#type: dist_obj
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("zip")
                .to_string(),
        };

        let require = entry
            .get("require")
            .and_then(|v| v.as_object())
            .map(|obj| {
                let mut map = RequireMap::new();
                for (k, v) in obj {
                    if let Some(s) = v.as_str() {
                        map.insert(k.clone(), s.to_string());
                    }
                }
                map
            })
            .unwrap_or_default();

        versions.push(PackageVersion {
            version,
            dist,
            require,
        });
    }

    Ok(PackageMetadata { versions })
}

// ---------------------------------------------------------------------------
// MockRegistry — for offline testing
// ---------------------------------------------------------------------------

/// In-process mock registry for tests. Pre-populate with `with_package`, then
/// use as a `Registry`.
#[derive(Default, Clone)]
pub struct MockRegistry {
    packages: std::sync::Arc<RwLock<HashMap<String, PackageMetadata>>>,
}

impl MockRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_package(self, full_name: &str, metadata: PackageMetadata) -> Self {
        self.packages
            .write()
            .insert(full_name.to_string(), metadata);
        self
    }
}

#[async_trait]
impl Registry for MockRegistry {
    async fn package_metadata(
        &self,
        vendor: &str,
        package: &str,
    ) -> Result<PackageMetadata, RegistryError> {
        let key = format!("{vendor}/{package}");
        self.packages
            .read()
            .get(&key)
            .cloned()
            .ok_or_else(|| RegistryError::NotFound(key))
    }
}
