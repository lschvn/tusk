//! `tusk-resolver` — wraps `pubgrub` with a Composer-flavored adapter.
//!
//! `pubgrub` gives us two headline features for free:
//!   1. fast, correct dependency resolution
//!   2. *human-readable* conflict explanations ("because X requires Y and Z
//!      requires not-Y, version solving failed") — the DX win GOAL.md §2
//!      calls out as a feature.

#![forbid(unsafe_code)]

mod adapter;
mod solver;

pub use adapter::ComposerVersion;
pub use solver::{ResolveError, ResolveOptions, ResolvedDependency, Resolver};
