//! Dependency resolver — greedy resolution with constraint intersection.
//!
//! Strategy: process the dependency queue breadth-first in **batches**. For
//! each layer, collect all un-fetched packages and call the registry's
//! `batch_package_metadata` once. Implementations with HTTP/2 multiplexing
//! (libcurl multi) collapse N round-trips into 1.
//!
//! This is simpler than full `PubGrub` backtracking but handles the vast majority
//! of real-world Composer dependency graphs correctly. Full `PubGrub` integration
//! is deferred until we need sophisticated conflict messages.

#![allow(clippy::all)]

use std::collections::{BTreeMap, VecDeque};

use thiserror::Error;
use tusk_manifest::RequireMap;
use tusk_registry::PackageVersion;
use tusk_registry::Registry;
use tusk_semver::{Constraint, Version};

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("version solving failed:\n{0}")]
    Conflict(String),
    #[error("package not found in registry: {0}")]
    NotFound(String),
    #[error("registry error for {package}: {source}")]
    Registry {
        package: String,
        #[source]
        source: tusk_registry::RegistryError,
    },
}

#[derive(Debug, Clone, Default)]
pub struct ResolveOptions {
    pub include_dev: bool,
    pub minimum_stability: String,
    pub prefer_stable: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedDependency {
    pub name: String,
    pub version: Version,
    pub require: RequireMap,
    pub dist: tusk_registry::DistRef,
}

pub struct Resolver<R: Registry> {
    registry: R,
}

impl<R: Registry> Resolver<R> {
    #[must_use]
    pub fn new(registry: R) -> Self {
        Self { registry }
    }

    /// Resolve all dependencies for the given root requirements.
    ///
    /// `root` = production deps from `require`.
    /// `dev` = dev deps from `require-dev` (only resolved if `opts.include_dev`).
    ///
    /// Metadata fetching is **batched** per BFS layer. For each layer, all
    /// un-fetched packages are sent to `batch_package_metadata` in a single
    /// call. With HTTP/2 multiplexing, this is ~1 round-trip per layer
    /// instead of N (one per package). For a 55-package project on a
    /// 100ms-RTT network, that's ~5.5s → ~400ms.
    #[allow(clippy::too_many_lines)]
    pub async fn resolve(
        &self,
        root: RequireMap,
        dev: RequireMap,
        opts: ResolveOptions,
    ) -> Result<Vec<ResolvedDependency>, ResolveError> {
        const BATCH_SIZE: usize = 64; // Max packages per layer (matches Bun's max-in-flight)

        // constraint_sources[pkg_name] = [(constraint_str, requesting_pkg)]
        let mut constraints: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        // BFS queue of packages to fetch metadata for
        let mut to_fetch: VecDeque<String> = VecDeque::new();
        // Resolved packages
        let mut resolved: BTreeMap<String, ResolvedDependency> = BTreeMap::new();

        // Seed with root + dev deps
        for (name, constraint) in &root {
            constraints
                .entry(name.clone())
                .or_default()
                .push((constraint.clone(), "<root>".to_string()));
            to_fetch.push_back(name.clone());
        }
        if opts.include_dev {
            for (name, constraint) in &dev {
                constraints
                    .entry(name.clone())
                    .or_default()
                    .push((constraint.clone(), "<root-dev>".to_string()));
                to_fetch.push_back(name.clone());
            }
        }

        // BFS: process the queue layer by layer, batch-fetching metadata.
        while !to_fetch.is_empty() {
            // Drain up to BATCH_SIZE packages that are neither resolved nor
            // platform-only requirements.
            let mut batch: Vec<(String, String)> = Vec::with_capacity(BATCH_SIZE);
            while batch.len() < BATCH_SIZE {
                let Some(p) = to_fetch.pop_front() else {
                    break;
                };
                if is_platform_requirement(&p) || resolved.contains_key(&p) {
                    continue;
                }
                // Skip duplicates within the current batch (cheap, the registry
                // will also dedup via its in-process cache).
                if batch.iter().any(|(v, pkg)| format!("{v}/{pkg}") == p) {
                    continue;
                }
                let (vendor, package) = split_name_owned(&p);
                batch.push((vendor, package));
            }

            if batch.is_empty() {
                continue;
            }

            // One batch call — HTTP/2 multiplexed when the registry supports it.
            let results = self
                .registry
                .batch_package_metadata(&batch)
                .await
                .map_err(|source| ResolveError::Registry {
                    package: "<batch>".to_string(),
                    source,
                })?;

            // Process each result.
            for ((vendor, package), metadata_result) in batch.into_iter().zip(results.into_iter())
            {
                let pkg_name = format!("{vendor}/{package}");

                let metadata = metadata_result.map_err(|source| ResolveError::Registry {
                    package: pkg_name.clone(),
                    source,
                })?;

                // Collect all constraints on this package
                let pkg_constraints = constraints.get(&pkg_name).cloned().unwrap_or_default();

                // Parse all constraints
                let parsed_constraints: Vec<(Constraint, String, String)> = pkg_constraints
                    .iter()
                    .map(|(cs, src)| match Constraint::parse(cs) {
                        Ok(c) => (c, cs.clone(), src.clone()),
                        Err(_) => (Constraint::parse("*").unwrap(), cs.clone(), src.clone()),
                    })
                    .collect();

                // Find highest version satisfying ALL constraints
                let mut candidates: Vec<&PackageVersion> = metadata
                    .versions
                    .iter()
                    .filter(|pv| {
                        parsed_constraints
                            .iter()
                            .all(|(c, _, _)| c.matches(&pv.version))
                    })
                    .collect();

                // Sort descending by version
                candidates.sort_by(|a, b| b.version.cmp_key().cmp(&a.version.cmp_key()));

                let Some(chosen) = candidates.first() else {
                    // No version satisfies all constraints → conflict
                    let constraint_summary = pkg_constraints
                        .iter()
                        .map(|(cs, src)| format!("  {src} requires {pkg_name}: {cs}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    let available = metadata
                        .versions
                        .iter()
                        .map(|pv| pv.version.to_composer_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(ResolveError::Conflict(format!(
                        "No version of {pkg_name} satisfies all constraints:\n{constraint_summary}\n\nAvailable versions: {available}"
                    )));
                };

                // Record resolution
                let resolved_dep = ResolvedDependency {
                    name: pkg_name.clone(),
                    version: chosen.version.clone(),
                    require: chosen.require.clone(),
                    dist: chosen.dist.clone(),
                };
                resolved.insert(pkg_name.clone(), resolved_dep);

                // Queue transitive dependencies
                for (dep_name, dep_constraint) in &chosen.require {
                    if is_platform_requirement(dep_name) {
                        continue;
                    }
                    if resolved.contains_key(dep_name) {
                        continue;
                    }
                    constraints
                        .entry(dep_name.clone())
                        .or_default()
                        .push((dep_constraint.clone(), pkg_name.clone()));
                    to_fetch.push_back(dep_name.clone());
                }
            }
        }

        // Convert to sorted vec (sorted by name for determinism)
        Ok(resolved.into_values().collect())
    }
}

/// Check if a requirement name is a platform requirement (not a real package).
/// Platform: `php`, `ext-*`, `lib-*`, `composer-plugin-api`, etc.
fn is_platform_requirement(name: &str) -> bool {
    name == "php"
        || name.starts_with("ext-")
        || name.starts_with("lib-")
        || name.starts_with("composer-")
        || !name.contains('/')
}

/// Owned version of `split_name` — returns owned strings so the result
/// can be moved into an `async move` block without borrow issues.
fn split_name_owned(full: &str) -> (String, String) {
    match full.split_once('/') {
        Some((v, p)) => (v.to_string(), p.to_string()),
        None => (full.to_string(), String::new()),
    }
}

/// Extension trait to get a comparison key for Version (avoids importing Ord).
trait VersionCmpKey {
    fn cmp_key(&self) -> (u32, u32, u32, u32, u8, u32);
}

impl VersionCmpKey for Version {
    fn cmp_key(&self) -> (u32, u32, u32, u32, u8, u32) {
        (
            self.major,
            self.minor,
            self.patch,
            self.tweak.unwrap_or(0),
            self.stability as u8,
            self.stability_n.unwrap_or(0),
        )
    }
}
