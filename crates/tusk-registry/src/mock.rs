//! Stub: filled in at Step 3 (TDD).
#![allow(dead_code, clippy::all)]

use std::collections::HashMap;

use async_trait::async_trait;
use parking_lot::Mutex;

use crate::client::{PackageMetadata, Registry, RegistryError};

/// In-process `Registry` for unit + integration tests. No HTTP.
#[derive(Default, Debug)]
pub struct MockRegistry {
    packages: Mutex<HashMap<String, PackageMetadata>>,
}

impl MockRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_package(self, name: &str, metadata: PackageMetadata) -> Self {
        self.packages.lock().insert(name.to_owned(), metadata);
        self
    }
}

#[async_trait]
impl Registry for MockRegistry {
    async fn package_metadata(
        &self,
        vendor: &str,
        package: &str,
    ) -> Result<PackageMetadata, RegistryError> {
        let key = format!("{vendor}/{package}");
        self.packages
            .lock()
            .get(&key)
            .cloned()
            .ok_or_else(|| RegistryError::NotFound(key))
    }
}
