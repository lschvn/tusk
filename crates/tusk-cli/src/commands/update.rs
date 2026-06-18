//! `tusk update` — re-resolve and re-install, ignoring the lock file.
#![allow(clippy::unused_async)]

use anyhow::Result;
use clap::Args;

#[derive(Args, Debug, Default)]
pub struct UpdateArgs {
    /// Skip dev dependencies.
    #[arg(long)]
    pub no_dev: bool,

    /// Operate quietly.
    #[arg(long, short)]
    pub quiet: bool,

    /// Update only the listed packages.
    #[arg(value_name = "PACKAGE")]
    pub packages: Vec<String>,
}

pub async fn run(_args: UpdateArgs) -> Result<()> {
    unimplemented!("tusk update — see GOAL.md §7.7")
}
