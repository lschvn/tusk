//! Stub cli module — replaced with the real clap-derive definition in Step 7.
#![allow(dead_code)]

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::commands::{install, remove, require, update};

#[derive(Parser, Debug)]
#[command(name = "tusk", version, about = "A fast PHP toolchain in Rust")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Resolve and install all dependencies declared in composer.json
    Install(install::InstallArgs),
    /// Update dependencies to the latest versions allowed by composer.json
    Update(update::UpdateArgs),
    /// Add a new package to composer.json and install it
    Require(require::RequireArgs),
    /// Remove a package from composer.json and vendor/
    Remove(remove::RemoveArgs),
}

impl Cli {
    pub async fn run(self) -> Result<()> {
        match self.command {
            Command::Install(args) => crate::commands::install::run(args).await,
            Command::Update(args) => crate::commands::update::run(args).await,
            Command::Require(args) => crate::commands::require::run(args).await,
            Command::Remove(args) => crate::commands::remove::run(args).await,
        }
    }
}
