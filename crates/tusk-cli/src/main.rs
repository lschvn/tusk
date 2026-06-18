//! `tusk` — the CLI binary.
//!
//! Subcommands (Phase 1): install, update, require, remove.
//! See GOAL.md §7.7 for the test-first behavior list.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod cli;
mod commands;
mod progress;
mod ui;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    cli::Cli::parse().run().await
}
