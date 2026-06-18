//! `composer.lock` data model — Composer-compatible serialization.
//!
//! Round-trip compatible: what we write, real Composer can read.
#![allow(clippy::all)]

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LockError {
    #[error("invalid composer.lock: {0}")]
    Json(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposerLock {
    #[serde(rename = "_readme", default, skip_serializing_if = "Option::is_none")]
    pub readme: Option<Vec<String>>,
    #[serde(
        rename = "content-hash",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub packages: Vec<LockedPackage>,
    #[serde(
        rename = "packages-dev",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub packages_dev: Vec<LockedPackage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<serde_json::Value>,
    #[serde(rename = "minimum-stability", default = "default_stability")]
    pub minimum_stability: String,
    #[serde(
        rename = "stability-flags",
        default,
        skip_serializing_if = "IndexMap::is_empty"
    )]
    pub stability_flags: IndexMap<String, u32>,
    #[serde(rename = "prefer-stable", default)]
    pub prefer_stable: bool,
    #[serde(rename = "prefer-lowest", default)]
    pub prefer_lowest: bool,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub platform: IndexMap<String, String>,
    #[serde(
        rename = "platform-dev",
        default,
        skip_serializing_if = "IndexMap::is_empty"
    )]
    pub platform_dev: IndexMap<String, String>,
    #[serde(
        rename = "plugin-api-version",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub plugin_api_version: Option<String>,
}

fn default_stability() -> String {
    "stable".to_string()
}

impl ComposerLock {
    /// Parse a `composer.lock` document from a JSON string.
    pub fn deserialize_str(s: &str) -> Result<Self, LockError> {
        serde_json::from_str(s).map_err(|e| LockError::Json(e.to_string()))
    }

    /// Serialize to a JSON string (pretty-printed, as Composer does).
    pub fn serialize_to_string(&self) -> Result<String, LockError> {
        serde_json::to_string_pretty(self).map_err(|e| LockError::Json(e.to_string()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPackage {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub source: serde_json::Value,
    pub dist: Dist,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub require: IndexMap<String, String>,
    #[serde(
        rename = "require-dev",
        default,
        skip_serializing_if = "IndexMap::is_empty"
    )]
    pub require_dev: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub conflict: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub replace: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub provide: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub suggest: IndexMap<String, String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_field: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub autoload: serde_json::Value,
    #[serde(
        rename = "autoload-dev",
        default,
        skip_serializing_if = "serde_json::Value::is_null"
    )]
    pub autoload_dev: serde_json::Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub license: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notification_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dist {
    pub url: String,
    pub r#type: String,
    #[serde(default)]
    pub shasum: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

pub type LockContentHash = String;
