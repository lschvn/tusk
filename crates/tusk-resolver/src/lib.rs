//! `tusk-resolver` — dependency resolution.
//!
//! Greedy resolver: picks highest matching version per package, recursively
//! resolves transitive deps, and detects conflicts when constraints are
//! incompatible.

#![forbid(unsafe_code)]

mod solver;

pub use solver::{ResolveError, ResolveOptions, ResolvedDependency, Resolver};
