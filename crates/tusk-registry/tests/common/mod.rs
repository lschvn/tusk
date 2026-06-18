//! Shared test helpers for the tusk-registry integration suite.

#![allow(dead_code)]

use tusk_manifest::RequireMap;
use tusk_registry::{DistRef, PackageMetadata, PackageVersion};
use tusk_semver::{Stability, Version};

/// Build a `Version` with the given major/minor/patch numbers, marked stable.
///
/// Avoids depending on `Version::parse` so tests stay decoupled from the
/// semver crate's parser completeness. When the semver crate stabilizes
/// stability-suffix parsing, additional helpers here can replace direct
/// field initialization.
#[must_use]
pub fn stable_version(major: u32, minor: u32, patch: u32) -> Version {
    Version {
        major,
        minor,
        patch,
        tweak: None,
        stability: Stability::Stable,
        stability_n: None,
        dev_branch: None,
        is_v_prefixed: false,
    }
}

/// Build an empty `RequireMap` (no dependencies).
#[must_use]
pub fn empty_require() -> RequireMap {
    RequireMap::new()
}

/// Build a `DistRef` pointing at a fake (but well-formed) zip URL and shasum.
#[must_use]
pub fn dist(url: &str, shasum: &str) -> DistRef {
    DistRef {
        url: url.to_owned(),
        shasum: shasum.to_owned(),
        r#type: "zip".to_owned(),
    }
}

/// Build a `PackageVersion` for a stable release with the given URL + shasum.
#[must_use]
pub fn version(major: u32, minor: u32, patch: u32, url: &str, shasum: &str) -> PackageVersion {
    PackageVersion {
        version: stable_version(major, minor, patch),
        dist: dist(url, shasum),
        require: empty_require(),
    }
}

/// Wrap a list of `PackageVersion`s in a `PackageMetadata`.
#[must_use]
pub fn metadata(versions: Vec<PackageVersion>) -> PackageMetadata {
    PackageMetadata { versions }
}
