//! Stub: filled in at Step 5 (TDD).
#![allow(dead_code, clippy::unused_async, clippy::all)]

use std::path::PathBuf;

use thiserror::Error;
use tusk_registry::Registry;
use tusk_resolver::ResolvedDependency;

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("install failed: {0}")]
    Failed(String),
}

pub struct Installer<R: Registry> {
    _registry: std::marker::PhantomData<R>,
    pub vendor_dir: PathBuf,
}

impl<R: Registry> Installer<R> {
    pub fn new(_registry: R, vendor_dir: PathBuf) -> Self {
        Self {
            _registry: std::marker::PhantomData,
            vendor_dir,
        }
    }
    pub async fn install_all(
        &self,
        _resolved: Vec<ResolvedDependency>,
    ) -> Result<(), InstallError> {
        unimplemented!()
    }
}
