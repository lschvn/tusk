//! libcurl multi-handle batch metadata fetcher with HTTP/2 multiplexing.
//!
//! Same technique as `tusk-installer::curl_downloader` but for Packagist
//! p2 metadata fetches. All requests to `repo.packagist.org` share a
//! single TCP+TLS+HTTP/2 connection, eliminating per-request TLS handshake
//! overhead during dependency resolution.
//!
//! The function is synchronous (libcurl is sync). The async `Registry`
//! trait wraps it via `tokio::task::spawn_blocking`.

use crate::client::PackageMetadata;
use crate::client::RegistryError;
use curl::easy::{Easy2, Handler, WriteError};
use curl::multi::Multi;
use std::time::Duration;
use thiserror::Error;

/// Errors from the curl metadata fetcher.
#[derive(Debug, Error)]
pub enum CurlMetadataError {
    /// A curl multi-handle operation failed.
    #[error("curl multi error: {0}")]
    Multi(String),
    /// A curl easy-handle operation failed.
    #[error("curl easy error: {0}")]
    Easy(String),
    /// HTTP status error.
    #[error("HTTP {status} for {url}")]
    HttpStatus {
        /// HTTP response status code.
        status: u32,
        /// The URL that failed.
        url: String,
    },
    /// JSON parse error.
    #[error("parse error for {url}: {source}")]
    Parse {
        /// The URL that failed to parse.
        url: String,
        /// The underlying parse error.
        #[source]
        source: RegistryError,
    },
}

impl From<CurlMetadataError> for RegistryError {
    fn from(e: CurlMetadataError) -> Self {
        match e {
            CurlMetadataError::Multi(s) | CurlMetadataError::Easy(s) => {
                RegistryError::Network(s)
            }
            CurlMetadataError::HttpStatus { status, url } => {
                RegistryError::Network(format!("HTTP {status} for {url}"))
            }
            CurlMetadataError::Parse { source, .. } => source,
        }
    }
}

/// Internal handler that collects the JSON response body.
struct Collector {
    /// Accumulated response bytes.
    data: Vec<u8>,
}

impl Collector {
    fn new() -> Self {
        Self {
            data: Vec::with_capacity(8 * 1024),
        }
    }
}

impl Handler for Collector {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        self.data.extend_from_slice(data);
        Ok(data.len())
    }
}

/// Configure a single easy handle for a metadata fetch.
fn configure_easy(easy: &mut Easy2<Collector>, url: &str) -> Result<(), CurlMetadataError> {
    easy.url(url)
        .map_err(|e| CurlMetadataError::Easy(e.to_string()))?;
    easy.useragent("tusk/0.1.0 (+https://github.com/lschvn/tusk)")
        .map_err(|e| CurlMetadataError::Easy(e.to_string()))?;
    // HTTP/2 over TLS via ALPN — multiplex with peers that support it.
    // Packagist (Cloudflare) supports h2, so this works.
    easy.http_version(curl::easy::HttpVersion::V2TLS)
        .map_err(|e| CurlMetadataError::Easy(e.to_string()))?;
    easy.timeout(Duration::from_secs(30))
        .map_err(|e| CurlMetadataError::Easy(e.to_string()))?;
    easy.connect_timeout(Duration::from_secs(10))
        .map_err(|e| CurlMetadataError::Easy(e.to_string()))?;
    easy.tcp_nodelay(true)
        .map_err(|e| CurlMetadataError::Easy(e.to_string()))?;
    Ok(())
}

/// Batch-fetch Packagist p2 metadata for many packages in parallel over a
/// single libcurl multi handle.
///
/// All requests to the same host share one TCP+TLS+HTTP/2 connection via
/// HTTP/2 stream multiplexing. For a BFS resolution layer of N packages,
/// this collapses N TLS handshakes into 1.
///
/// `package_keys` is a list of `"vendor/package"` strings (the p2 cache key).
/// `base_url` is the Packagist base (no trailing slash).
///
/// Returns one result per input, in order.
pub fn fetch_batch(
    base_url: &str,
    package_keys: &[String],
    parse_fn: impl Fn(&str, &serde_json::Value) -> Result<PackageMetadata, RegistryError>,
) -> Result<Vec<Result<PackageMetadata, CurlMetadataError>>, CurlMetadataError> {
    if package_keys.is_empty() {
        return Ok(Vec::new());
    }

    let mut multi = Multi::new();

    // Enable HTTP/2 multiplexing — critical for Packagist.
    multi
        .pipelining(false, true)
        .map_err(|e| CurlMetadataError::Multi(e.to_string()))?;

    // Force one connection per host: all metadata fetches to repo.packagist.org
    // share a single TCP+TLS connection via HTTP/2 streams.
    multi
        .set_max_host_connections(1)
        .map_err(|e| CurlMetadataError::Multi(e.to_string()))?;

    multi
        .set_max_concurrent_streams(100)
        .map_err(|e| CurlMetadataError::Multi(e.to_string()))?;

    let mut handles: Vec<(curl::multi::Easy2Handle<Collector>, usize)> =
        Vec::with_capacity(package_keys.len());

    for (i, key) in package_keys.iter().enumerate() {
        let url = format!("{base_url}/p2/{key}.json");
        let mut easy = Easy2::new(Collector::new());
        configure_easy(&mut easy, &url)?;
        let handle = multi
            .add2(easy)
            .map_err(|e| CurlMetadataError::Multi(e.to_string()))?;
        handles.push((handle, i));
    }

    // Poll until all transfers complete.
    loop {
        let still_running = multi
            .perform()
            .map_err(|e| CurlMetadataError::Multi(e.to_string()))?;

        if still_running == 0 {
            break;
        }

        multi
            .wait(&mut [], Duration::from_millis(10))
            .map_err(|e| CurlMetadataError::Multi(e.to_string()))?;
    }

    // Collect results.
    let mut results: Vec<Result<PackageMetadata, CurlMetadataError>> = (0..package_keys.len())
        .map(|_| {
            Err(CurlMetadataError::Easy(
                "not completed".to_string(),
            ))
        })
        .collect();

    for (handle, i) in handles {
        let mut easy = multi
            .remove2(handle)
            .map_err(|e| CurlMetadataError::Multi(e.to_string()))?;

        let key = &package_keys[i];
        let url = format!("{base_url}/p2/{key}.json");

        match easy.response_code() {
            Ok(404) => {
                // 404 → package not found (matches the reqwest client behavior)
                results[i] = Err(CurlMetadataError::Parse {
                    url,
                    source: RegistryError::NotFound(key.clone()),
                });
            }
            Ok(status) if (200..400).contains(&status) => {
                let data = std::mem::take(&mut easy.get_mut().data);
                match serde_json::from_slice::<serde_json::Value>(&data) {
                    Ok(json) => match parse_fn(key, &json) {
                        Ok(meta) => results[i] = Ok(meta),
                        Err(e) => {
                            results[i] = Err(CurlMetadataError::Parse { url, source: e });
                        }
                    },
                    Err(e) => {
                        results[i] = Err(CurlMetadataError::Parse {
                            url,
                            source: RegistryError::Parse(e.to_string()),
                        });
                    }
                }
            }
            Ok(status) => {
                results[i] = Err(CurlMetadataError::HttpStatus { status, url });
            }
            Err(e) => {
                results[i] = Err(CurlMetadataError::Easy(e.to_string()));
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_batch() {
        let result = fetch_batch("https://example.com", &[], |_k, _v| {
            Ok(PackageMetadata { versions: vec![] })
        })
        .unwrap();
        assert!(result.is_empty());
    }
}
