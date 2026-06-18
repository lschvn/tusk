//! `tusk` — the CLI binary.
//!
//! Subcommands (Phase 1): install, update, require, remove.
//! See GOAL.md §7.7 for the test-first behavior list.

#![forbid(unsafe_code)]

mod cli;
mod commands;
mod progress;
mod ui;

pub use cli::Cli;
