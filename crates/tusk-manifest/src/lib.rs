//! `tusk-manifest` — `composer.json` and `composer.lock` parse + serialize.
//!
//! The goal is round-trip compatibility: a `composer.lock` tusk writes must
//! be readable by real Composer, and a `composer.json` tusk reads must cover
//! the wild (Laravel, Symfony, a tiny lib).

#![forbid(unsafe_code)]

mod composer_json;
mod composer_lock;

pub use composer_json::{Autoload, AutoloadDev, ComposerJson, RequireMap};
pub use composer_lock::{ComposerLock, Dist, LockContentHash, LockedPackage};
