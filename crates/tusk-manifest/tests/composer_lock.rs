//! Integration tests for `composer.lock` parse + serialize round-trip.
//!
//! A `composer.lock` that tusk writes MUST be readable by real Composer.
//! The strongest guarantee is a parse → serialize → parse round-trip producing
//! the same structured data.

use tusk_manifest::ComposerLock;

const SIMPLE_LOCK: &str = include_str!("../../../fixtures/manifest/simple-lock.json");

#[test]
fn lock_from_str_parses_packages() {
    let lock = ComposerLock::deserialize_str(SIMPLE_LOCK).expect("simple-lock.json should parse");

    assert_eq!(lock.packages.len(), 1);
    let pkg = &lock.packages[0];
    assert_eq!(pkg.name, "acme/foo");
    assert_eq!(pkg.version, "1.2.3");
    assert_eq!(pkg.dist.url, "https://example.com/acme-foo-1.2.3.zip");
    assert_eq!(pkg.dist.shasum, "abc123def456");
    assert_eq!(pkg.dist.r#type, "zip");
}

#[test]
fn lock_roundtrip_preserves_structure() {
    let lock = ComposerLock::deserialize_str(SIMPLE_LOCK).expect("should parse");
    let serialized = lock.serialize_to_string().expect("should serialize");
    let reparsed = ComposerLock::deserialize_str(&serialized).expect("should re-parse");

    assert_eq!(lock.packages.len(), reparsed.packages.len());
    assert_eq!(lock.packages[0].name, reparsed.packages[0].name);
    assert_eq!(lock.packages[0].version, reparsed.packages[0].version);
    assert_eq!(
        lock.packages[0].dist.shasum,
        reparsed.packages[0].dist.shasum
    );
}

#[test]
fn lock_serialization_includes_required_fields() {
    let lock = ComposerLock::deserialize_str(SIMPLE_LOCK).expect("should parse");
    let json = lock.serialize_to_string().expect("should serialize");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("should be valid JSON");

    assert!(parsed.get("packages").is_some(), "packages must be present");
    assert!(
        parsed.get("content-hash").is_some(),
        "content-hash must be present"
    );
}
