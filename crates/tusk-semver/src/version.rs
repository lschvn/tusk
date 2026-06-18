//! Stub: filled in at Step 1 (TDD).
#![allow(dead_code, clippy::all)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VersionError {
    #[error("invalid version string: {0}")]
    Invalid(String),
}

/// Composer's stability flags, in version-order (lower = "less stable").
///
/// Per the Composer versions spec, dev < alpha < beta < RC < stable.
/// Numeric "patch" numbers only matter within the same stability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Stability {
    Dev,
    Alpha,
    Beta,
    Rc,
    Stable,
}

impl Stability {
    pub fn from_suffix(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "dev" => Some(Self::Dev),
            "a" | "alpha" => Some(Self::Alpha),
            "b" | "beta" => Some(Self::Beta),
            "rc" | "p" => Some(Self::Rc),
            "pl" | "patch" => Some(Self::Stable),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Alpha => "alpha",
            Self::Beta => "beta",
            Self::Rc => "RC",
            Self::Stable => "stable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    /// Optional 4th numeric component (rare in the wild; Composer supports it).
    pub tweak: Option<u32>,
    pub stability: Stability,
    /// Stability-suffix number (e.g. `1.2.3-beta.2` -> 2). `None` => 0.
    pub stability_n: Option<u32>,
    /// `dev-<branch>` form, e.g. `dev-main`, `dev-feature/x`.
    pub dev_branch: Option<String>,
    /// Optional short alias ("v" prefix tolerated on parse; stripped on display).
    pub is_v_prefixed: bool,
}

impl Version {
    pub fn parse(s: &str) -> Result<Self, VersionError> {
        let (is_v_prefixed, body) = if let Some(stripped) = s.strip_prefix('v') {
            (true, stripped)
        } else if let Some(stripped) = s.strip_prefix('V') {
            (true, stripped)
        } else {
            (false, s)
        };
        let mut parts = body.split('.');
        let major: u32 = parts
            .next()
            .and_then(|p| p.parse().ok())
            .ok_or_else(|| VersionError::Invalid(s.to_string()))?;
        let minor: u32 = parts
            .next()
            .and_then(|p| p.parse().ok())
            .ok_or_else(|| VersionError::Invalid(s.to_string()))?;
        let patch: u32 = parts
            .next()
            .and_then(|p| p.parse().ok())
            .ok_or_else(|| VersionError::Invalid(s.to_string()))?;
        let tweak: Option<u32> = parts
            .next()
            .map(|p| p.parse())
            .transpose()
            .map_err(|_| VersionError::Invalid(s.to_string()))?;
        if parts.next().is_some() {
            return Err(VersionError::Invalid(s.to_string()));
        }
        Ok(Self {
            major,
            minor,
            patch,
            tweak,
            stability: Stability::Stable,
            stability_n: None,
            dev_branch: None,
            is_v_prefixed,
        })
    }

    pub fn to_composer_string(&self) -> String {
        match self.tweak {
            Some(t) => format!("{}.{}.{}.{}", self.major, self.minor, self.patch, t),
            None => format!("{}.{}.{}", self.major, self.minor, self.patch),
        }
    }
}
