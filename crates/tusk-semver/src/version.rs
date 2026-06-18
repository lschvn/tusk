//! Composer version + constraint parser. See GOAL.md §7.1.
#![allow(clippy::all)]

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

        // Split into numeric prefix and optional `-<stability>...` suffix.
        // The first `-` separates the numeric part from the stability tail.
        let (numeric_part, stability_tail) = match body.find('-') {
            Some(idx) => (&body[..idx], Some(&body[idx + 1..])),
            None => (body, None),
        };

        // Parse numeric prefix X[.Y[.Z[.T]]]
        // Composer allows short forms: `2.0` and `2` are valid (the missing
        // parts default to 0). The constraint `^2.0` therefore matches 2.0.0+.
        let mut parts = numeric_part.split('.');
        let major: u32 = parts
            .next()
            .and_then(|p| p.parse().ok())
            .ok_or_else(|| VersionError::Invalid(s.to_string()))?;
        // Parse up to 3 more components; missing ones default to 0.
        let minor: u32 = parts
            .next()
            .map(str::parse)
            .transpose()
            .map_err(|_| VersionError::Invalid(s.to_string()))?
            .unwrap_or(0);
        let patch: u32 = parts
            .next()
            .map(str::parse)
            .transpose()
            .map_err(|_| VersionError::Invalid(s.to_string()))?
            .unwrap_or(0);
        let tweak: Option<u32> = parts
            .next()
            .map(str::parse)
            .transpose()
            .map_err(|_| VersionError::Invalid(s.to_string()))?;
        if parts.next().is_some() {
            return Err(VersionError::Invalid(s.to_string()));
        }

        // Parse stability suffix, if any.
        let (stability, stability_n) = match stability_tail {
            Some(suf) => {
                parse_stability_suffix(suf).ok_or_else(|| VersionError::Invalid(s.to_string()))?
            }
            None => (Stability::Stable, None),
        };

        Ok(Self {
            major,
            minor,
            patch,
            tweak,
            stability,
            stability_n,
            dev_branch: None,
            is_v_prefixed,
        })
    }

    pub fn to_composer_string(&self) -> String {
        let mut s = match self.tweak {
            Some(t) => format!("{}.{}.{}.{}", self.major, self.minor, self.patch, t),
            None => format!("{}.{}.{}", self.major, self.minor, self.patch),
        };
        if self.stability != Stability::Stable || self.stability_n.is_some() {
            s.push('-');
            s.push_str(self.stability.as_str());
            if let Some(n) = self.stability_n {
                s.push('.');
                s.push_str(&n.to_string());
            }
        }
        s
    }
}

/// Parse a stability tail like `alpha`, `alpha.2`, `RC1`, `dev`, `pl3`.
fn parse_stability_suffix(suf: &str) -> Option<(Stability, Option<u32>)> {
    if suf.is_empty() {
        return None;
    }
    // Find the boundary where the leading alphabetic run ends.
    let letters_end = suf
        .find(|c: char| c.is_ascii_digit() || c == '.')
        .unwrap_or(suf.len());
    let (letters, rest) = suf.split_at(letters_end);
    let stab = Stability::from_suffix(letters)?;
    let number = if rest.is_empty() {
        None
    } else if let Some(after_dot) = rest.strip_prefix('.') {
        if after_dot.is_empty() || !after_dot.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        Some(after_dot.parse().ok()?)
    } else if rest.chars().all(|c| c.is_ascii_digit()) {
        Some(rest.parse().ok()?)
    } else {
        return None;
    };
    Some((stab, number))
}
