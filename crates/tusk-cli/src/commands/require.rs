//! `tusk require <pkg>` — add to composer.json, re-resolve, install.
#![allow(clippy::unused_async)]

use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct RequireArgs {
    /// Package(s) to require, in `vendor/package` or `vendor/package:^1.0` form.
    #[arg(value_name = "PACKAGE", required = true)]
    pub packages: Vec<String>,

    /// Add to require-dev instead of require.
    #[arg(long)]
    pub dev: bool,

    /// Operate quietly.
    #[arg(long, short)]
    pub quiet: bool,
}

pub async fn run(_args: RequireArgs) -> Result<()> {
    unimplemented!("tusk require — see GOAL.md §7.7")
}
