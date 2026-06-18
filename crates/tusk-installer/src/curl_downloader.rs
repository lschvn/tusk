//! libcurl multi-handle batch downloader with HTTP/2 multiplexing.
//!
//! All downloads to the same host share a single TCP+TLS connection via
//! HTTP/2 stream multiplexing. This eliminates per-request TLS handshake
//! overhead — the dominant cost when downloading many small archives from
//! `codeload.github.com`.
//!
//! Bun achieves its speed partly through this technique. libcurl's `multi`
//! interface is purpose-built for batch parallel transfers with automatic
//! connection sharing and HTTP/2 multiplexing.

use curl::easy::{Easy2, Handler, HttpVersion, WriteError};
use curl::multi::Multi;
use std::time::Duration;
use thiserror::Error;

/// Errors from the curl downloader.
#[derive(Debug, Error)]
pub enum CurlError {
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
}

/// Internal handler that collects the response body.
struct Collector {
    /// Accumulated response bytes.
    data: Vec<u8>,
}

impl Collector {
    fn new() -> Self {
        Self {
            data: Vec::with_capacity(64 * 1024),
        }
    }
}

impl Handler for Collector {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        self.data.extend_from_slice(data);
        Ok(data.len())
    }
}

/// Configure a single easy handle for a download.
fn configure_easy(easy: &mut Easy2<Collector>, url: &str) -> Result<(), CurlError> {
    easy.url(url)
        .map_err(|e| CurlError::Easy(e.to_string()))?;
    easy.useragent("tusk/0.1.0 (+https://github.com/lschvn/tusk)")
        .map_err(|e| CurlError::Easy(e.to_string()))?;
    // HTTP/2 over TLS via ALPN — the key to multiplexing
    easy.http_version(HttpVersion::V2TLS)
        .map_err(|e| CurlError::Easy(e.to_string()))?;
    easy.follow_location(true)
        .map_err(|e| CurlError::Easy(e.to_string()))?;
    easy.max_redirections(5)
        .map_err(|e| CurlError::Easy(e.to_string()))?;
    easy.timeout(Duration::from_secs(60))
        .map_err(|e| CurlError::Easy(e.to_string()))?;
    easy.tcp_keepalive(true)
        .map_err(|e| CurlError::Easy(e.to_string()))?;
    easy.tcp_nodelay(true)
        .map_err(|e| CurlError::Easy(e.to_string()))?;
    easy.connect_timeout(Duration::from_secs(10))
        .map_err(|e| CurlError::Easy(e.to_string()))?;
    Ok(())
}

/// Batch-download multiple URLs in parallel using a single libcurl multi handle.
///
/// All requests to the same host are automatically multiplexed over a single
/// HTTP/2 connection. Returns one result per input URL, in order.
///
/// # Errors
///
/// Returns `Err` only if the multi-handle setup fails catastrophically.
/// Individual transfer failures are reported as `Err` in the corresponding
/// result slot.
pub fn download_batch(urls: &[String]) -> Result<Vec<Result<Vec<u8>, CurlError>>, CurlError> {
    if urls.is_empty() {
        return Ok(Vec::new());
    }

    let mut multi = Multi::new();

    // Enable HTTP/2 multiplexing — multiple requests share one connection.
    // This is the critical setting that makes libcurl dramatically faster
    // than reqwest for batch downloads from the same host.
    multi
        .pipelining(false, true)
        .map_err(|e| CurlError::Multi(e.to_string()))?;

    // Force multiplexing: limit connections PER HOST to 2.
    // With HTTP/2, each connection can carry 100 multiplexed streams,
    // so 2 connections × 100 streams = 200 concurrent downloads per host.
    // Without this limit, curl opens one connection per request, negating
    // the multiplexing benefit entirely.
    multi
        .set_max_host_connections(1)
        .map_err(|e| CurlError::Multi(e.to_string()))?;

    // Allow up to 100 concurrent HTTP/2 streams per connection.
    multi
        .set_max_concurrent_streams(100)
        .map_err(|e| CurlError::Multi(e.to_string()))?;

    // Track handles with their original URL index.
    let mut handles: Vec<(curl::multi::Easy2Handle<Collector>, usize)> =
        Vec::with_capacity(urls.len());

    for (i, url) in urls.iter().enumerate() {
        let mut easy = Easy2::new(Collector::new());
        configure_easy(&mut easy, url)?;
        let handle = multi
            .add2(easy)
            .map_err(|e| CurlError::Multi(e.to_string()))?;
        handles.push((handle, i));
    }

    // Poll until all transfers complete.
    loop {
        let still_running = multi
            .perform()
            .map_err(|e| CurlError::Multi(e.to_string()))?;

        if still_running == 0 {
            break;
        }

        // Wait for socket activity (up to 10ms).
        multi
            .wait(&mut [], Duration::from_millis(10))
            .map_err(|e| CurlError::Multi(e.to_string()))?;
    }

    // Collect results from each handle.
    let mut results: Vec<Result<Vec<u8>, CurlError>> = (0..urls.len())
        .map(|_| Err(CurlError::Easy("not completed".to_string())))
        .collect();

    for (handle, i) in handles {
        let mut easy = multi
            .remove2(handle)
            .map_err(|e| CurlError::Multi(e.to_string()))?;

        match easy.response_code() {
            Ok(status) if (200..400).contains(&status) => {
                let data = std::mem::take(&mut easy.get_mut().data);
                results[i] = Ok(data);
            }
            Ok(status) => {
                results[i] = Err(CurlError::HttpStatus {
                    status,
                    url: urls[i].clone(),
                });
            }
            Err(e) => {
                results[i] = Err(CurlError::Easy(e.to_string()));
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
        let results = download_batch(&[]).unwrap();
        assert!(results.is_empty());
    }
}
