//! Stub: filled in at Step 2 (TDD).
#![allow(dead_code, clippy::all)]

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("invalid composer.json: {0}")]
    Json(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposerJson {
    pub name: Option<String>,
    #[serde(default)]
    pub require: RequireMap,
    #[serde(rename = "require-dev", default)]
    pub require_dev: RequireMap,
    #[serde(default)]
    pub autoload: Autoload,
    #[serde(rename = "autoload-dev", default)]
    pub autoload_dev: AutoloadDev,
    #[serde(default)]
    pub repositories: Vec<serde_json::Value>,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(rename = "minimum-stability", default)]
    pub minimum_stability: Option<String>,
    #[serde(rename = "prefer-stable", default)]
    pub prefer_stable: bool,
}

pub type RequireMap = IndexMap<String, String>;
pub type Autoload = indexmap::IndexMap<String, serde_json::Value>;
pub type AutoloadDev = indexmap::IndexMap<String, serde_json::Value>;

impl ComposerJson {
    /// Parse a `composer.json` document from a string.
    ///
    /// Forward-compatible: unknown top-level fields are ignored, and any of the
    /// known sections may be absent. Invalid JSON is the only error case.
    pub fn from_str(s: &str) -> Result<Self, ManifestError> {
        serde_json::from_str(s).map_err(|e| ManifestError::Json(e.to_string()))
    }
}
