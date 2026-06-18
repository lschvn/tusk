//! `tusk install` — resolve deps, download, extract, write lock + autoloader.
#![allow(clippy::unused_async)]

use anyhow::Result;
use clap::Args;

#[derive(Args, Debug, Default)]
pub struct InstallArgs {
    /// Skip dev dependencies (require-dev).
    #[arg(long)]
    pub no_dev: bool,

    /// Operate quietly (no progress UI).
    #[arg(long, short)]
    pub quiet: bool,

    /// Read platform requirements (PHP version, extensions) from this JSON
    /// file instead of detecting a PHP install. Required in Phase 1.
    #[arg(long)]
    pub platform: Option<std::path::PathBuf>,
}

pub async fn run(_args: InstallArgs) -> Result<()> {
    // Filled in at Step 7 (TDD).
    unimplemented!("tusk install — see GOAL.md §7.7 and ROADMAP.md Step 7")
}
