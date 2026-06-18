//! `tusk-registry` — Packagist client behind a `Registry` trait.
//!
//! The trait boundary lets the resolver, installer, and CLI tests run fully
//! offline against `wiremock` mocks or an in-process `MockRegistry`. Per
//! GOAL.md §5, no test in the workspace may hit the real network.

#![forbid(unsafe_code)]

mod client;

pub use client::{
    DistRef, MockRegistry, PackageMetadata, PackageVersion, PackagistClient, Registry,
    RegistryError,
};
