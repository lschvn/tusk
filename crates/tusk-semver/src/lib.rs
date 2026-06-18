//! `tusk-semver` — Composer-flavored version + constraint grammar.
//!
//! See GOAL.md §4, §7.1. This is the algorithmic heart of tusk: every other
//! crate that touches versions or constraints leans on this one.

#![forbid(unsafe_code)]

mod constraint;
mod version;

pub use constraint::{Constraint, ConstraintParser};
pub use version::{Stability, Version};
