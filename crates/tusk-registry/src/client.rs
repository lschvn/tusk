//! Stub: filled in at Step 3 (TDD).
#![allow(dead_code, clippy::all)]

use async_trait::async_trait;
use thiserror::Error;
use tusk_manifest::RequireMap;
use tusk_semver::Version;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("network error: {0}")]
    Network(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("package not found: {0}")]
    NotFound(String),
}

#[async_trait]
pub trait Registry: Send + Sync {
    /// Fetch all version metadata for a package, with in-process caching.
    async fn package_metadata(
        &self,
        vendor: &str,
        package: &str,
    ) -> Result<PackageMetadata, RegistryError>;
}

#[derive(Debug, Clone)]
pub struct PackageMetadata {
    pub versions: Vec<PackageVersion>,
}

#[derive(Debug, Clone)]
pub struct PackageVersion {
    pub version: Version,
    pub dist: DistRef,
    pub require: RequireMap,
}

#[derive(Debug, Clone)]
pub struct DistRef {
    pub url: String,
    pub shasum: String,
    pub r#type: String,
}

/// Convenience alias used by the resolver: a string constraint on a package version.
pub type VersionConstraint = String;

#[derive(Clone)]
pub struct PackagistClient {
    base_url: String,
}

impl PackagistClient {
    pub fn new(_base_url: impl Into<String>) -> Self {
        Self {
            base_url: String::new(),
        }
    }
}

#[async_trait]
impl Registry for PackagistClient {
    async fn package_metadata(
        &self,
        _vendor: &str,
        _package: &str,
    ) -> Result<PackageMetadata, RegistryError> {
        unimplemented!()
    }
}
