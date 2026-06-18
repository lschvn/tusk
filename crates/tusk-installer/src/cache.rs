//! Stub: filled in at Step 5 (TDD).
#![allow(dead_code, clippy::all)]

use std::path::PathBuf;

pub struct Cache {
    pub root: PathBuf,
}

impl Cache {
    pub fn new(_root: PathBuf) -> Self {
        Self { root: PathBuf::new() }
    }
}
