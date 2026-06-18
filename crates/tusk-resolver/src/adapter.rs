//! Stub: filled in at Step 4 (TDD).
#![allow(dead_code, clippy::all)]

use tusk_semver::Version;

/// Adapter: a `Version` that pubgrub can sort and range over.
///
/// (In Step 4 we wire this up to pubgrub's `VersionLike` trait.)
#[derive(Debug, Clone)]
pub struct ComposerVersion(pub Version);
