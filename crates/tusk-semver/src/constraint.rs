//! Stub: filled in at Step 1 (TDD).
#![allow(dead_code, clippy::all)]

use crate::Version;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConstraintError {
    #[error("invalid constraint: {0}")]
    Invalid(String),
}

/// One OR-branch of a Composer constraint expression.
///
/// `1.2 || >=2.0 <3.0` is the disjunction of `Constraint { OR: [Branch(1.2), Branch(>=2.0 <3.0)] }`
/// where the right branch is itself the conjunction of two `RangeOp`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constraint {
    /// All branches are OR'd. A single-branch constraint is the common case.
    pub branches: Vec<Branch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    /// AND'd atomic constraints within this branch. Empty = "match anything".
    pub atoms: Vec<Atom>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Atom {
    /// `1.2.3` (exact), `1.2.*`, `*`
    Exact(Version),
    /// `^1.2`, `^1.2.3`
    Caret { lower: Version },
    /// `~1.2`, `~1.2.3`
    Tilde { lower: Version },
    /// `>=`, `>`, `<=`, `<`, `!=`
    Cmp { op: CmpOp, version: Version },
    /// `1.2 - 3.4` (hyphen range)
    Hyphen { lower: Version, upper: Version },
    /// `@dev`, `@stable`, `@alpha`, etc.
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

impl Constraint {
    pub fn parse(_s: &str) -> Result<Self, ConstraintError> {
        unimplemented!("see Step 1 tests")
    }

    pub fn matches(&self, _version: &Version) -> bool {
        unimplemented!("see Step 1 tests")
    }
}

pub struct ConstraintParser;
impl ConstraintParser {
    pub fn parse(_s: &str) -> Result<Constraint, ConstraintError> {
        unimplemented!()
    }
}
