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
    /// Fetch all version metadata for a single package.
    ///
    /// Default implementation batches a single-element request via
    /// `batch_package_metadata`. Implementations may override for efficiency.
    async fn package_metadata(
        &self,
        vendor: &str,
        package: &str,
    ) -> Result<PackageMetadata, RegistryError> {
        let results = self
            .batch_package_metadata(&[(vendor.to_string(), package.to_string())])
            .await?;
        results
            .into_iter()
            .next()
            .unwrap_or_else(|| Err(RegistryError::NotFound(format!("{vendor}/{package}"))))
    }

    /// Batch-fetch metadata for many packages in parallel.
    ///
    /// Implementations should use HTTP/2 multiplexing when possible so a
    /// BFS resolution layer of N packages takes ~1 round-trip to fetch.
    /// The default implementation spawns N parallel `package_metadata` calls,
    /// which is correct but not multiplexed.
    async fn batch_package_metadata(
        &self,
        packages: &[(String, String)],
    ) -> Result<Vec<Result<PackageMetadata, RegistryError>>, RegistryError> {
        let mut results = Vec::with_capacity(packages.len());
        for (vendor, package) in packages {
            let r = self.package_metadata(vendor, package).await;
            results.push(r);
        }
        Ok(results)
    }
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

    /// Batch-fetch via libcurl multi-handle with HTTP/2 multiplexing.
    ///
    /// This is the speed-critical path for cold installs without a lockfile.
    /// Instead of N parallel reqwest requests (each opens a separate TLS
    /// connection), we make one curl multi call that multiplexes all
    /// requests over a single HTTP/2 connection to `repo.packagist.org`.
    async fn batch_package_metadata(
        &self,
        packages: &[(String, String)],
    ) -> Result<Vec<Result<PackageMetadata, RegistryError>>, RegistryError> {
        if packages.is_empty() {
            return Ok(Vec::new());
        }

        // Build full keys and check cache.
        let keys: Vec<String> = packages
            .iter()
            .map(|(v, p)| format!("{v}/{p}"))
            .collect();

        let mut to_fetch: Vec<(usize, String)> = Vec::new(); // (original_index, key)
        let mut results: Vec<Option<PackageMetadata>> = (0..keys.len()).map(|_| None).collect();

        {
            let cache = self.cache.read();
            for (i, key) in keys.iter().enumerate() {
                if let Some(meta) = cache.get(key) {
                    results[i] = Some(meta.clone());
                } else {
                    to_fetch.push((i, key.clone()));
                }
            }
        }

        if to_fetch.is_empty() {
            return Ok(results
                .into_iter()
                .map(|r| r.ok_or_else(|| RegistryError::Parse("missing".to_string())))
                .collect());
        }

        // Fetch all cache misses in a single libcurl multi call (HTTP/2 multiplexed).
        let fetch_keys: Vec<String> = to_fetch.iter().map(|(_, k)| k.clone()).collect();
        let base_url = self.base_url.clone();

        let fetched = tokio::task::spawn_blocking(move || {
            crate::curl_metadata::fetch_batch(&base_url, &fetch_keys, parse_p2_response)
        })
        .await
        .map_err(|e| RegistryError::Network(format!("join error: {e}")))?
        .map_err(|e| RegistryError::Network(e.to_string()))?;

        // Store successful results in cache and fill in the output vec.
        let mut cache_writes = Vec::new();
        for (slot, fetch_result) in to_fetch.iter().zip(fetched.into_iter()) {
            let (orig_idx, key) = slot;
            if let Ok(meta) = fetch_result {
                cache_writes.push((key.clone(), meta.clone()));
                results[*orig_idx] = Some(meta);
            }
            // Err: leave as None; will become Err in the output map below
        }
        if !cache_writes.is_empty() {
            let mut cache = self.cache.write();
            for (k, m) in cache_writes {
                cache.insert(k, m);
            }
        }

        Ok(results
            .into_iter()
            .enumerate()
            .map(|(i, r)| {
                r.ok_or_else(|| {
                    // Reconstruct the key for the error
                    let key = keys
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| "<unknown>".to_string());
                    RegistryError::Parse(format!("metadata for {key} not fetched"))
                })
            })
            .collect())
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
