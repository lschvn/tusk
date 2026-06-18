//! Regression tests for the p2 metadata parser and HTTP client.
//!
//! These cover two real bugs the benchmark suite surfaced:
//!
//! 1. **Missing User-Agent** — GitHub's codeload endpoint requires a
//!    User-Agent header; reqwest's default is rejected with HTTP 403.
//! 2. **Dev-branch versions** — real Packagist responses include
//!    `dev-*` versions with no `dist` field. The parser must skip
//!    them (they are source-only installs, out of Phase 1 scope) rather
//!    than aborting the whole package.

#![allow(clippy::pedantic)]

use tusk_registry::{PackagistClient, Registry};
use wiremock::matchers::{header_regex, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn packagist_client_sends_user_agent_header() {
    // The server will assert the request carries our User-Agent.
    // If the header is missing or wrong, wiremock will return 404 and the test fails.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/p2/acme/foo.json"))
        .and(header_regex("User-Agent", r"^tusk/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "packages": {
                "acme/foo": [
                    {
                        "version": "1.0.0",
                        "dist": {
                            "url": "https://example.com/foo.zip",
                            "shasum": "abc",
                            "type": "zip"
                        },
                        "require": {}
                    }
                ]
            }
        })))
        .mount(&server)
        .await;

    let client = PackagistClient::new(server.uri());
    let result = client.package_metadata("acme", "foo").await;

    assert!(
        result.is_ok(),
        "PackagistClient must send User-Agent header; got: {result:?}"
    );
    assert_eq!(result.unwrap().versions.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn packagist_parser_skips_dev_branches_without_dist() {
    // Real Packagist responses for packages like illuminate/database or
    // symfony/framework-bundle include `dev-main`, `dev-master`, etc.,
    // which have a `source` field but NO `dist` field. The parser must
    // skip these (source-only installs are out of scope for Phase 1).
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/p2/acme/foo.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "packages": {
                "acme/foo": [
                    {
                        "version": "1.0.0",
                        "dist": {
                            "url": "https://example.com/foo-1.0.0.zip",
                            "shasum": "aaa",
                            "type": "zip"
                        },
                        "require": {}
                    },
                    {
                        "version": "dev-main",
                        "source": {
                            "type": "git",
                            "url": "https://github.com/acme/foo.git",
                            "reference": "main"
                        }
                        // NOTE: no `dist` field — source-only
                    },
                    {
                        "version": "1.5.0",
                        "dist": {
                            "url": "https://example.com/foo-1.5.0.zip",
                            "shasum": "bbb",
                            "type": "zip"
                        },
                        "require": {}
                    }
                ]
            }
        })))
        .mount(&server)
        .await;

    let client = PackagistClient::new(server.uri());
    let result = client.package_metadata("acme", "foo").await;

    let meta = result.expect("parser must not error on missing dist");
    // Only the two versions WITH dist should be kept; dev-main skipped.
    assert_eq!(
        meta.versions.len(),
        2,
        "expected dev-main to be skipped, got {} versions: {:?}",
        meta.versions.len(),
        meta.versions
            .iter()
            .map(|v| format!(
                "{}.{}.{}",
                v.version.major, v.version.minor, v.version.patch
            ))
            .collect::<Vec<_>>()
    );
    // Verify the kept versions are 1.0.0 and 1.5.0
    let majors: Vec<u32> = meta.versions.iter().map(|v| v.version.major).collect();
    assert!(majors.contains(&1));
    assert!(
        !meta.versions.iter().any(|v| v.version.major == 0),
        "dev-main should not be present as a stable version"
    );
}
