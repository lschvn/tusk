//! Stub: filled in at Step 4 (TDD).
#![allow(dead_code, clippy::all)]

use thiserror::Error;
use tusk_manifest::RequireMap;
use tusk_registry::Registry;
use tusk_semver::Version;

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("version solving failed:\n{0}")]
    Conflict(String),
}

#[derive(Debug, Clone, Default)]
pub struct ResolveOptions {
    pub include_dev: bool,
    pub minimum_stability: String,
    pub prefer_stable: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedDependency {
    pub name: String,
    pub version: Version,
    pub require: RequireMap,
}

pub struct Resolver<R: Registry> {
    _registry: std::marker::PhantomData<R>,
}

impl<R: Registry> Resolver<R> {
    pub fn new(_registry: R) -> Self {
        Self {
            _registry: std::marker::PhantomData,
        }
    }
    pub fn resolve(
        &self,
        _root: RequireMap,
        _dev: RequireMap,
        _opts: ResolveOptions,
    ) -> Result<Vec<ResolvedDependency>, ResolveError> {
        unimplemented!()
    }
}
