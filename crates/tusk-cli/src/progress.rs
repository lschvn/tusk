//! `indicatif`-backed progress reporting shared by subcommands.
#![allow(dead_code)]

use indicatif::{ProgressBar, ProgressStyle};

pub fn spinner(msg: &str, quiet: bool) -> ProgressBar {
    if quiet {
        return ProgressBar::hidden();
    }
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
    );
    pb.set_message(msg.to_owned());
    pb
}

pub fn bar(len: u64, msg: &str, quiet: bool) -> ProgressBar {
    if quiet {
        return ProgressBar::hidden();
    }
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::with_template("{msg} [{bar:30.cyan/blue}] {pos}/{len} {eta_precise}")
            .unwrap()
            .progress_chars("=>-"),
    );
    pb.set_message(msg.to_owned());
    pb
}
