//! Composer constraint grammar. Spec: <https://getcomposer.org/doc/articles/versions.md>.
//!
//! Public API (fixed in the scaffold) preserved; bodies implemented.
#![allow(clippy::all)]

use crate::version::Version;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConstraintError {
    #[error("invalid constraint: {0}")]
    Invalid(String),
}

/// Disjunction of conjunctions:
///   `1.2 || >=2.0 <3.0` is `Constraint { branches: [Branch(1.2), Branch(>=2.0 <3.0)] }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constraint {
    pub branches: Vec<Branch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    /// AND'd atomic constraints. Empty = matches anything.
    pub atoms: Vec<Atom>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Atom {
    Exact(Version),
    Caret { lower: Version },
    Tilde { lower: Version },
    Cmp { op: CmpOp, version: Version },
    Hyphen { lower: Version, upper: Version },
    StabilityFlag(StabilityFlag),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Gt,
    Ge,
    Lt,
    Le,
    Ne,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StabilityFlag {
    Dev,
    Alpha,
    Beta,
    Rc,
    Stable,
}

impl StabilityFlag {
    fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "dev" => Some(Self::Dev),
            "alpha" | "a" => Some(Self::Alpha),
            "beta" | "b" => Some(Self::Beta),
            "rc" | "p" => Some(Self::Rc),
            "stable" => Some(Self::Stable),
            _ => None,
        }
    }

    fn as_stability(self) -> crate::Stability {
        match self {
            Self::Dev => crate::Stability::Dev,
            Self::Alpha => crate::Stability::Alpha,
            Self::Beta => crate::Stability::Beta,
            Self::Rc => crate::Stability::Rc,
            Self::Stable => crate::Stability::Stable,
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

impl Constraint {
    pub fn parse(s: &str) -> Result<Self, ConstraintError> {
        let input = s.trim();
        if input.is_empty() {
            return Ok(Self {
                branches: vec![Branch { atoms: vec![] }],
            });
        }
        let branch_strs = split_or_branches(input);
        let mut branches = Vec::with_capacity(branch_strs.len());
        for b in branch_strs {
            let trimmed = b.trim();
            if trimmed == "*" {
                branches.push(Branch { atoms: vec![] });
            } else {
                let atoms = parse_branch(trimmed)?;
                branches.push(Branch { atoms });
            }
        }
        if branches.is_empty() {
            return Err(ConstraintError::Invalid(s.to_string()));
        }
        Ok(Self { branches })
    }

    pub fn matches(&self, version: &Version) -> bool {
        self.branches.iter().any(|branch| branch.matches(version))
    }
}

impl Branch {
    fn matches(&self, version: &Version) -> bool {
        if self.atoms.is_empty() {
            return true;
        }
        self.atoms.iter().all(|atom| atom.matches(version))
    }
}

impl Atom {
    fn matches(&self, version: &Version) -> bool {
        match self {
            Self::Exact(v) => version_eq_stable(version, v),
            Self::Caret { lower } => matches_caret(version, lower),
            Self::Tilde { lower } => matches_tilde(version, lower),
            Self::Cmp { op, version: v } => matches_cmp(version, *op, v),
            Self::Hyphen { lower, upper } => {
                // Composer's hyphen range `A - B` is `[A, next(B))`, where
                // `next(B)` increments the LAST component of B and zeroes
                // the rest. e.g. `1.2 - 3.4` => [1.2.0, 3.5.0).
                let upper = next_upper_bound(upper);
                version_cmp(version, lower) != std::cmp::Ordering::Less
                    && version_cmp(version, &upper) == std::cmp::Ordering::Less
            }
            Self::StabilityFlag(flag) => version.stability == flag.as_stability(),
        }
    }
}

/// Increment the last numeric component of `v` by 1, zero the rest.
/// `next(3.4.0)` = `3.5.0`; `next(3.4.5)` = `3.4.6`; `next(3.4)` = `3.5.0`.
fn next_upper_bound(v: &Version) -> Version {
    if v.patch > 0 || v.tweak.is_some() {
        Version {
            patch: v.patch + 1,
            tweak: None,
            ..v.clone()
        }
    } else if v.minor > 0 {
        Version {
            minor: v.minor + 1,
            patch: 0,
            tweak: None,
            ..v.clone()
        }
    } else {
        Version {
            major: v.major + 1,
            minor: 0,
            patch: 0,
            tweak: None,
            ..v.clone()
        }
    }
}

// ---------------------------------------------------------------------------
// Splitting helpers
// ---------------------------------------------------------------------------

fn split_or_branches(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'|' && bytes[i + 1] == b'|' {
            out.push(&s[start..i]);
            start = i + 2;
            i = start;
        } else {
            i += 1;
        }
    }
    out.push(&s[start..]);
    out
}

/// Parse a single branch (a conjunction of atoms) into a `Vec<Atom>`.
fn parse_branch(s: &str) -> Result<Vec<Atom>, ConstraintError> {
    let mut atoms = Vec::new();
    let mut rest = s;

    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        // Hyphen range: `1.2 - 3.4`
        if let Some((hyphen, after)) = try_parse_hyphen(rest)? {
            atoms.push(hyphen);
            rest = after;
            continue;
        }
        if rest.starts_with("||") {
            return Err(ConstraintError::Invalid(s.to_string()));
        }
        // Bare `@flag`
        if let Some(stripped) = rest.strip_prefix('@') {
            let name_end = stripped
                .find(|c: char| !c.is_ascii_alphabetic())
                .unwrap_or(stripped.len());
            let (name, after) = stripped.split_at(name_end);
            let flag = StabilityFlag::from_name(name)
                .ok_or_else(|| ConstraintError::Invalid(s.to_string()))?;
            atoms.push(Atom::StabilityFlag(flag));
            rest = after;
            continue;
        }
        // Single atom
        let (atom_str_raw, after_atom) = split_atom_and_separator(rest);
        let atom_str = atom_str_raw.trim();
        if atom_str.is_empty() {
            return Err(ConstraintError::Invalid(s.to_string()));
        }
        let mut sub_atoms = parse_single_atom(atom_str)?;
        let trailing_flag = sub_atoms
            .iter()
            .position(|a| matches!(a, Atom::StabilityFlag(_)))
            .map(|i| sub_atoms.remove(i));
        atoms.append(&mut sub_atoms);
        if let Some(flag_atom) = trailing_flag {
            atoms.push(flag_atom);
        }
        // Consume leading whitespace and a single comma (if present).
        let trimmed = after_atom.trim_start();
        if let Some(after_comma) = trimmed.strip_prefix(',') {
            rest = after_comma.trim_start();
        } else {
            rest = trimmed;
        }
    }

    Ok(atoms)
}

fn try_parse_hyphen(s: &str) -> Result<Option<(Atom, &str)>, ConstraintError> {
    let Some(left) = read_one_version_string(s) else {
        return Ok(None);
    };
    let after_left = s[left.len()..].trim_start();
    let Some(after_dash) = after_left.strip_prefix('-') else {
        return Ok(None);
    };
    let after_dash = after_dash.trim_start();
    if !after_dash.starts_with(|c: char| c.is_ascii_digit() || c == 'v' || c == 'V' || c == '*') {
        return Ok(None);
    }
    let Some(right) = read_one_version_string(after_dash) else {
        return Ok(None);
    };
    let lower = parse_version_str(left)?;
    let upper = parse_version_str(right)?;
    Ok(Some((
        Atom::Hyphen { lower, upper },
        &after_dash[right.len()..],
    )))
}

fn read_one_version_string(s: &str) -> Option<&str> {
    let end = s
        .find(|c: char| c.is_whitespace() || c == ',' || c == '-' || c == '@')
        .unwrap_or(s.len());
    if end == 0 {
        None
    } else {
        Some(&s[..end])
    }
}

fn split_atom_and_separator(s: &str) -> (&str, &str) {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_whitespace() || c == ',' {
            break;
        }
        i += 1;
    }
    (&s[..i], &s[i..])
}

/// Returns a sequence of atoms. For a wildcard (e.g. `1.2.*`), returns two:
/// one `Cmp { Ge, lower }` and one `Cmp { Lt, upper }` (Composer's wildcard
/// is strict-less at the upper bound). For a normal atom, returns one.
/// For a stability-flagged atom (e.g. `1.2.3@dev`), the flag is the LAST
/// element of the returned vec — `parse_branch` re-orders it to the end
/// of the branch's atom list.
fn parse_single_atom(s: &str) -> Result<Vec<Atom>, ConstraintError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(ConstraintError::Invalid(s.to_string()));
    }

    // Split off a trailing `@flag` (rfind: handles `1.2.3@dev`).
    let (core, flag) = if let Some(idx) = s.rfind('@') {
        let (a, b) = s.split_at(idx);
        let flag_str = &b[1..];
        let flag = StabilityFlag::from_name(flag_str);
        (a.to_string(), flag)
    } else {
        (s.to_string(), None)
    };
    let core = core.trim();

    let mut atoms: Vec<Atom> = Vec::new();
    if let Some(stripped) = core.strip_prefix("==") {
        atoms.push(Atom::Exact(parse_version_str(stripped)?));
    } else if let Some(stripped) = core.strip_prefix("!=") {
        atoms.push(Atom::Cmp {
            op: CmpOp::Ne,
            version: parse_version_str(stripped)?,
        });
    } else if let Some(stripped) = core.strip_prefix(">=") {
        atoms.push(Atom::Cmp {
            op: CmpOp::Ge,
            version: parse_version_str(stripped)?,
        });
    } else if let Some(stripped) = core.strip_prefix('>') {
        atoms.push(Atom::Cmp {
            op: CmpOp::Gt,
            version: parse_version_str(stripped)?,
        });
    } else if let Some(stripped) = core.strip_prefix("<=") {
        atoms.push(Atom::Cmp {
            op: CmpOp::Le,
            version: parse_version_str(stripped)?,
        });
    } else if let Some(stripped) = core.strip_prefix('<') {
        atoms.push(Atom::Cmp {
            op: CmpOp::Lt,
            version: parse_version_str(stripped)?,
        });
    } else if let Some(stripped) = core.strip_prefix('^') {
        atoms.push(Atom::Caret {
            lower: parse_version_str(stripped)?,
        });
    } else if let Some(stripped) = core.strip_prefix('~') {
        atoms.push(Atom::Tilde {
            lower: parse_version_str(stripped)?,
        });
    } else if core == "*" {
        // Handled in parse_branch as an empty branch. Reach here only if a
        // caller bypasses that; emit a Ge(0,0,0) atom which always matches.
        atoms.push(Atom::Cmp {
            op: CmpOp::Ge,
            version: version_of(0, 0, 0),
        });
    } else if let Some(prefix) = core.strip_suffix(".*") {
        // `1.*` => >=1.0.0, <2.0.0
        // `1.2.*` => >=1.2.0, <1.3.0  (Composer's wildcard is strict-less at upper)
        let parts: Vec<u32> = prefix
            .split('.')
            .map(|p| p.parse::<u32>().ok())
            .collect::<Option<_>>()
            .ok_or_else(|| ConstraintError::Invalid(s.to_string()))?;
        let (lower, upper) = match parts.as_slice() {
            [maj] => (version_of(*maj, 0, 0), version_of(maj + 1, 0, 0)),
            [maj, min] => (version_of(*maj, *min, 0), version_of(*maj, min + 1, 0)),
            _ => return Err(ConstraintError::Invalid(s.to_string())),
        };
        atoms.push(Atom::Cmp {
            op: CmpOp::Ge,
            version: lower,
        });
        atoms.push(Atom::Cmp {
            op: CmpOp::Lt,
            version: upper,
        });
    } else {
        atoms.push(Atom::Exact(parse_version_str(core)?));
    }

    if let Some(flag) = flag {
        // For an Exact atom, bake the stability directly into the version so
        // that version_cmp matches correctly. For range atoms (Caret, Tilde,
        // Cmp, Hyphen), add a separate StabilityFlag atom to AND-filter.
        if atoms.len() == 1 && matches!(atoms[0], Atom::Exact(_)) {
            if let Atom::Exact(ref mut v) = atoms[0] {
                v.stability = flag.as_stability();
            }
        } else {
            atoms.push(Atom::StabilityFlag(flag));
        }
    }

    Ok(atoms)
}

// ---------------------------------------------------------------------------
// Version helpers
// ---------------------------------------------------------------------------

fn parse_version_str(s: &str) -> Result<Version, ConstraintError> {
    if s == "*" {
        return Ok(version_of(0, 0, 0));
    }
    // Composer allows 2-part versions in constraints (e.g. `^1.2`, `~1.2`,
    // the bounds of `1.2 - 3.4`). Pad with `.0` to make them 3-part.
    let padded = pad_short_version(s);
    Version::parse(&padded).map_err(|_| ConstraintError::Invalid(s.to_string()))
}

/// Pad `X` or `X.Y` with `.0` to make `X.0.0` / `X.Y.0`. Leaves 3+ parts
/// alone. Doesn't touch strings that contain a non-numeric prefix like `v`.
fn pad_short_version(s: &str) -> String {
    if s.starts_with('v') || s.starts_with('V') || s.starts_with('*') {
        return s.to_string();
    }
    // Stop at the first non-digit/dot character (e.g. `@`, `-`, ` `).
    let boundary = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    let (head, tail) = s.split_at(boundary);
    let dots = head.matches('.').count();
    let pad = if dots == 0 {
        ".0.0"
    } else if dots == 1 {
        ".0"
    } else {
        ""
    };
    format!("{head}{pad}{tail}")
}

fn version_of(major: u32, minor: u32, patch: u32) -> Version {
    Version {
        major,
        minor,
        patch,
        tweak: None,
        stability: crate::Stability::Stable,
        stability_n: None,
        dev_branch: None,
        is_v_prefixed: false,
    }
}

// ---------------------------------------------------------------------------
// Matching helpers
// ---------------------------------------------------------------------------

fn version_eq_stable(v: &Version, target: &Version) -> bool {
    // An exact match requires identical numeric AND stability components.
    version_cmp(v, target) == std::cmp::Ordering::Equal
}

fn matches_cmp(v: &Version, op: CmpOp, target: &Version) -> bool {
    let ord = version_cmp(v, target);
    match op {
        CmpOp::Gt => ord == std::cmp::Ordering::Greater,
        CmpOp::Ge => ord != std::cmp::Ordering::Less,
        CmpOp::Lt => ord == std::cmp::Ordering::Less,
        CmpOp::Le => ord != std::cmp::Ordering::Greater,
        CmpOp::Ne => ord != std::cmp::Ordering::Equal,
    }
}

fn matches_caret(v: &Version, lower: &Version) -> bool {
    let upper = if lower.major > 0 {
        version_of(lower.major + 1, 0, 0)
    } else if lower.minor > 0 {
        version_of(0, lower.minor + 1, 0)
    } else {
        version_of(0, 0, lower.patch + 1)
    };
    version_cmp(v, lower) != std::cmp::Ordering::Less
        && version_cmp(v, &upper) == std::cmp::Ordering::Less
}

fn matches_tilde(v: &Version, lower: &Version) -> bool {
    let upper = if lower.patch > 0 || lower.tweak.is_some() {
        version_of(lower.major, lower.minor + 1, 0)
    } else {
        version_of(lower.major + 1, 0, 0)
    };
    version_cmp(v, lower) != std::cmp::Ordering::Less
        && version_cmp(v, &upper) == std::cmp::Ordering::Less
}

fn version_cmp(a: &Version, b: &Version) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let by_num = (a.major, a.minor, a.patch, a.tweak.unwrap_or(0)).cmp(&(
        b.major,
        b.minor,
        b.patch,
        b.tweak.unwrap_or(0),
    ));
    if by_num != Ordering::Equal {
        return by_num;
    }
    let by_stab = a.stability.cmp(&b.stability);
    if by_stab != Ordering::Equal {
        return by_stab;
    }
    a.stability_n.unwrap_or(0).cmp(&b.stability_n.unwrap_or(0))
}

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub struct ConstraintParser;
impl ConstraintParser {
    pub fn parse(s: &str) -> Result<Constraint, ConstraintError> {
        Constraint::parse(s)
    }
}
