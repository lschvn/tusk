//! Tests for the in-process `MockRegistry`.
//!
//! The mock is the foundation for every resolver / installer / CLI test in
//! the workspace, so the simplest behavior — "what I put in is what I get out"
//! — is pinned first.

mod common;

use common::{metadata, version};
use tusk_registry::{MockRegistry, Registry};

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
