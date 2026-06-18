//! `tusk-autoload` — generate Composer-compatible autoloader files.
//!
//! These PHP files are the contract with PHP frameworks. The output must
//! be byte-stable (sorted maps, deterministic ordering) so we can ship
//! golden snapshot tests (see GOAL.md §7.6).

#![forbid(unsafe_code)]

mod autoload_php;
mod classmap;
mod generator;
mod php_writer;

pub use autoload_php::AutoloadPhp;
pub use classmap::ClassMap;
pub use generator::{AutoloadGenerator, AutoloadSpec};
