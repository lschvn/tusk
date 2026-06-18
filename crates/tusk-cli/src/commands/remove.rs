//! `tusk remove <pkg>` — drop from composer.json, prune from vendor/, update lock.
#![allow(clippy::unused_async)]

use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
pub struct RemoveArgs {
    /// Package(s) to remove.
    #[arg(value_name = "PACKAGE", required = true)]
    pub packages: Vec<String>,

    /// Operate quietly.
    #[arg(long, short)]
    pub quiet: bool,
}

pub async fn run(_args: RemoveArgs) -> Result<()> {
    unimplemented!("tusk remove — see GOAL.md §7.7")
}
