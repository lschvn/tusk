//! Concurrency tests for the in-process `MockRegistry`.
//!
//! The trait demands `Send + Sync`. These tests pin that the mock is
//! usable from multiple tokio tasks simultaneously without data races
//! (verified by all tasks observing the same inserted metadata).

mod common;

use std::sync::Arc;

use common::{metadata, version};
use tusk_registry::{MockRegistry, Registry};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mock_registry_supports_concurrent_fetch() {
    // Arrange: one shared registry, one inserted package.
    let pkg = metadata(vec![version(
        2,
        0,
        0,
        "https://example.com/concurrent-2.0.0.zip",
        "deadbeef",
    )]);
    let registry = Arc::new(MockRegistry::new().with_package("acme/shared", pkg));

    // Act: spawn N tasks that all fetch the same package.
    let mut handles = Vec::new();
    for _ in 0..8 {
        let reg = Arc::clone(&registry);
        handles.push(tokio::spawn(async move {
            reg.package_metadata("acme", "shared").await
        }));
    }

    // Assert: every task got the same metadata back.
    for h in handles {
        let md = h
            .await
            .expect("task must not panic")
            .expect("mock should return inserted package");
        assert_eq!(md.versions.len(), 1);
        assert_eq!(md.versions[0].version.major, 2);
        assert_eq!(md.versions[0].dist.shasum, "deadbeef");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mock_registry_compiles_as_send_sync() {
    // This is a static-type assertion: if MockRegistry weren't Send + Sync,
    // we couldn't pass it across an async task boundary. We exercise the
    // trait through a boxed trait object too, since the resolver uses
    // `Arc<dyn Registry>`.
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}
    assert_send_sync::<MockRegistry>();
    assert_send_sync::<dyn Registry>();
}
