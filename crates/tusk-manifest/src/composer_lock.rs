//! Stub: filled in at Step 2 (TDD).
#![allow(dead_code, clippy::all)]

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposerLock {
    #[serde(rename = "_readme")]
    pub readme: Option<Vec<String>>,
    pub content_hash: Option<String>,
    pub packages: Vec<LockedPackage>,
    #[serde(rename = "packages-dev", default)]
    pub packages_dev: Vec<LockedPackage>,
    pub aliases: IndexMap<String, String>,
    #[serde(rename = "minimum-stability")]
    pub minimum_stability: String,
    #[serde(rename = "stability-flags", default)]
    pub stability_flags: IndexMap<String, String>,
    #[serde(rename = "prefer-stable", default)]
    pub prefer_stable: bool,
    #[serde(rename = "prefer-lowest", default)]
    pub prefer_lowest: bool,
    pub platform: IndexMap<String, String>,
    #[serde(rename = "platform-dev", default)]
    pub platform_dev: IndexMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPackage {
    pub name: String,
    pub version: String,
    pub source: serde_json::Value,
    pub dist: Dist,
    #[serde(default)]
    pub require: IndexMap<String, String>,
    #[serde(rename = "require-dev", default)]
    pub require_dev: IndexMap<String, String>,
    #[serde(default)]
    pub conflict: IndexMap<String, String>,
    #[serde(default)]
    pub replace: IndexMap<String, String>,
    #[serde(default)]
    pub provide: IndexMap<String, String>,
    #[serde(default)]
    pub suggest: IndexMap<String, String>,
    #[serde(default)]
    pub type_field: Option<String>,
    #[serde(default)]
    pub autoload: serde_json::Value,
    #[serde(default)]
    pub autoload_dev: serde_json::Value,
    #[serde(default)]
    pub license: Vec<String>,
    #[serde(default)]
    pub authors: Vec<serde_json::Value>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dist {
    pub url: String,
    pub r#type: String,
    #[serde(default)]
    pub shasum: String,
    #[serde(default)]
    pub reference: Option<String>,
}

pub type LockContentHash = String;
