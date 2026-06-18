//! Tests for the in-process `MockRegistry`.
//!
//! The mock is the foundation for every resolver / installer / CLI test in
//! the workspace, so the simplest behavior — "what I put in is what I get out"
//! — is pinned first.

mod common;

use common::{metadata, version};
use tusk_registry::{MockRegistry, Registry, RegistryError};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mock_registry_returns_inserted_metadata() {
    // Arrange: a registry with one package inserted under "acme/foo".
    let pkg = metadata(vec![version(
        1,
        2,
        3,
        "https://example.com/acme-foo-1.2.3.zip",
        "abc123",
    )]);
    let registry = MockRegistry::new().with_package("acme/foo", pkg);

    // Act: fetch the metadata we just inserted.
    let fetched = registry
        .package_metadata("acme", "foo")
        .await
        .expect("mock should return inserted package");

    // Assert: the version we put in is the version we got out, byte for byte.
    assert_eq!(fetched.versions.len(), 1);
    let v = &fetched.versions[0];
    assert_eq!(v.version.major, 1);
    assert_eq!(v.version.minor, 2);
    assert_eq!(v.version.patch, 3);
    assert_eq!(v.version.stability, tusk_semver::Stability::Stable);
    assert_eq!(v.dist.url, "https://example.com/acme-foo-1.2.3.zip");
    assert_eq!(v.dist.shasum, "abc123");
    assert_eq!(v.dist.r#type, "zip");
    assert!(v.require.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mock_registry_returns_not_found_for_unknown_package() {
    // Arrange: an empty registry.
    let registry = MockRegistry::new();

    // Act: fetch a package that was never inserted.
    let err = registry
        .package_metadata("unknown", "ghost")
        .await
        .expect_err("unknown package must error");

    // Assert: the error is a typed `NotFound`, not a panic or generic Err.
    match err {
        RegistryError::NotFound(name) => assert_eq!(name, "unknown/ghost"),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mock_registry_distinguishes_vendor_and_package_names() {
    // Arrange: only "acme/foo" is inserted.
    let pkg = metadata(vec![version(1, 0, 0, "https://example.com/foo.zip", "h1")]);
    let registry = MockRegistry::new().with_package("acme/foo", pkg);

    // Act + Assert: a different vendor with the same package name is unknown.
    let err = registry
        .package_metadata("othercorp", "foo")
        .await
        .expect_err("different vendor must be unknown");
    assert!(matches!(err, RegistryError::NotFound(name) if name == "othercorp/foo"));

    // Act + Assert: the same vendor with a different package name is unknown.
    let err = registry
        .package_metadata("acme", "bar")
        .await
        .expect_err("different package must be unknown");
    assert!(matches!(err, RegistryError::NotFound(name) if name == "acme/bar"));
}
