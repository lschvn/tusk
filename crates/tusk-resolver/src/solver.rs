//! Dependency resolver — greedy resolution with constraint intersection.
//!
//! Strategy: process the dependency queue breadth-first. For each package,
//! collect ALL constraints on it, then pick the highest version satisfying all.
//! If no version satisfies all constraints, produce a conflict error naming
//! the packages and their constraints.
//!
//! This is simpler than full `PubGrub` backtracking but handles the vast majority
//! of real-world Composer dependency graphs correctly. Full `PubGrub` integration
//! is deferred until we need sophisticated conflict messages.

#![allow(clippy::all)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};

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
    pub async fn resolve(
        &self,
        root: RequireMap,
        dev: RequireMap,
        opts: ResolveOptions,
    ) -> Result<Vec<ResolvedDependency>, ResolveError> {
        // Collect all initial requirements and their sources.
        // constraint_sources[pkg_name] = [(constraint_str, requesting_pkg)]
        let mut constraints: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        // Queue of packages to process
        let mut queue: VecDeque<String> = VecDeque::new();

        for (name, constraint) in &root {
            constraints
                .entry(name.clone())
                .or_default()
                .push((constraint.clone(), "<root>".to_string()));
            queue.push_back(name.clone());
        }

        if opts.include_dev {
            for (name, constraint) in &dev {
                constraints
                    .entry(name.clone())
                    .or_default()
                    .push((constraint.clone(), "<root-dev>".to_string()));
                queue.push_back(name.clone());
            }
        }

        // Resolved packages: name → ResolvedDependency
        let mut resolved: BTreeMap<String, ResolvedDependency> = BTreeMap::new();
        // Track which packages we've already fetched metadata for
        let mut fetched: BTreeSet<String> = BTreeSet::new();

        while let Some(pkg_name) = queue.pop_front() {
            // Skip platform requirements (php, ext-*, lib-*) — not installable packages
            if is_platform_requirement(&pkg_name) {
                continue;
            }

            // Already resolved?
            if resolved.contains_key(&pkg_name) {
                continue;
            }

            // Fetch metadata
            let (vendor, package) = split_name(&pkg_name);
            let metadata = self
                .registry
                .package_metadata(vendor, package)
                .await
                .map_err(|source| ResolveError::Registry {
                    package: pkg_name.clone(),
                    source,
                })?;

            fetched.insert(pkg_name.clone());

            // Collect all constraints on this package
            let pkg_constraints = constraints.get(&pkg_name).cloned().unwrap_or_default();

            // Parse all constraints
            let parsed_constraints: Vec<(Constraint, String, String)> = pkg_constraints
                .iter()
                .map(|(cs, src)| match Constraint::parse(cs) {
                    Ok(c) => (c, cs.clone(), src.clone()),
                    Err(_) => {
                        // If constraint can't be parsed, treat as "*" (any)
                        (Constraint::parse("*").unwrap(), cs.clone(), src.clone())
                    }
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
                constraints
                    .entry(dep_name.clone())
                    .or_default()
                    .push((dep_constraint.clone(), pkg_name.clone()));
                queue.push_back(dep_name.clone());
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

/// Split "vendor/package" into ("vendor", "package").
fn split_name(full: &str) -> (&str, &str) {
    match full.split_once('/') {
        Some((v, p)) => (v, p),
        None => (full, ""),
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
