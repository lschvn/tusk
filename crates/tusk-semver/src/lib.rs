//! `tusk-semver` — Composer-flavored version + constraint grammar.
//!
//! See GOAL.md §4, §7.1. This is the algorithmic heart of tusk: every other
//! crate that touches versions or constraints leans on this one.

#![forbid(unsafe_code)]

mod version;
mod constraint;

pub use version::{Stability, Version};
pub use constraint::{Constraint, ConstraintParser};
