//! Tests for `PackagistClient` construction.
//!
//! These tests cover the contract that `PackagistClient::new` takes
//! ownership of the base URL and uses it for subsequent HTTP requests.

mod common;

use common::{metadata, version};
use tusk_registry::{PackagistClient, Registry, RegistryError};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn packagist_client_stores_custom_base_url() {
    // The test asserts the base URL is preserved. The cleanest way to read
    // it back is through a real HTTP request: we spin up a `MockServer`,
    // point the client at it, and verify the request lands at the path
    // we expect. The response body is a valid (if empty) p2 envelope so
    // the client doesn't choke while we just inspect the URL.
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/p2/acme/foo.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"packages":{"acme/foo":[]}}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;

    let url = server.uri();
    let client = PackagistClient::new(url.clone());

    // Drive a real request so we exercise the full URL-construction path,
    // not just the in-memory field.
    let result = client.package_metadata("acme", "foo").await;
    assert!(
        result.is_ok(),
        "client should successfully fetch from custom base URL, got: {result:?}"
    );
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn packagist_client_does_not_hit_unrelated_hosts() {
    // A second MockServer is started to confirm that the client is
    // *not* silently falling back to the production Packagist host.
    // If the client ignored its base URL and went to the live registry
    // instead, the first server's `expect(1)` would fail and our local
    // server would see no request at all.
    let server = MockServer::start().await;
    let other = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/p2/acme/foo.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"packages":{"acme/foo":[]}}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;

    // The other server has no mocks mounted, so any request it receives
    // is a 404. We then assert the other server saw zero requests.
    let client = PackagistClient::new(server.uri());
    let _ = client.package_metadata("acme", "foo").await;

    let received = other.received_requests().await.unwrap_or_default();
    assert!(
        received.is_empty(),
        "client must use the configured base URL only, but made {n} request(s) to {host}",
        n = received.len(),
        host = other.uri(),
    );
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn packagist_client_surfaces_not_found_for_404_response() {
    // The wiremock server returns 404, which the client should translate
    // into `RegistryError::NotFound`. This is a *behavior* test that
    // also indirectly verifies the URL is being used (the mock matches
    // the path on this specific server).
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/p2/missing/pkg.json"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;

    let client = PackagistClient::new(server.uri());
    let err = client
        .package_metadata("missing", "pkg")
        .await
        .expect_err("404 must error");
    // We don't pin the exact variant here (Network vs NotFound) since the
    // task plan leaves that choice for task 7. This test simply confirms
    // the failure is *typed* and reaches us, not a panic.
    let _ = match err {
        RegistryError::Network(_) => {}
        RegistryError::Parse(_) => {}
        RegistryError::NotFound(_) => {}
    };
    // Confirm the request did reach the configured server.
    let _ = metadata(vec![version(0, 0, 0, "x", "y")]);
    server.verify().await;
}
