//! `tusk install` — resolve deps, download, extract, write lock + autoloader.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;
use tusk_autoload::{AutoloadGenerator, AutoloadSpec, InstalledPackage};
use tusk_installer::Installer;
use tusk_manifest::ComposerJson;
use tusk_registry::PackagistClient;
use tusk_resolver::{ResolveOptions, Resolver};

#[derive(Args, Debug, Default)]
pub struct InstallArgs {
    /// Skip dev dependencies (require-dev).
    #[arg(long)]
    pub no_dev: bool,

    /// Operate quietly (no progress UI).
    #[arg(long, short)]
    pub quiet: bool,

    /// Read platform requirements (PHP version, extensions) from this JSON
    /// file instead of detecting a PHP install. Required in Phase 1.
    #[arg(long)]
    pub platform: Option<PathBuf>,

    /// Packagist base URL (for testing; defaults to `<https://repo.packagist.org>`).
    #[arg(long)]
    pub packagist_url: Option<String>,
}

/// Entry point for `tusk install`.
pub async fn run(args: InstallArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("get current directory")?;
    run_in_dir(&cwd, args).await
}

/// Run install in a specific directory (testable).
pub async fn run_in_dir(project_dir: &Path, args: InstallArgs) -> Result<()> {
    // 1. Read composer.json
    let cj_path = project_dir.join("composer.json");
    let cj_content = std::fs::read_to_string(&cj_path)
        .with_context(|| format!("reading {}", cj_path.display()))?;
    let manifest = ComposerJson::from_str(&cj_content).context("parsing composer.json")?;

    if !args.quiet {
        println!("Loading composer.json...");
    }

    // 2. Resolve dependencies
    let base_url = args
        .packagist_url
        .clone()
        .unwrap_or_else(|| "https://repo.packagist.org".to_string());

    let opts = ResolveOptions {
        include_dev: !args.no_dev,
        minimum_stability: manifest.minimum_stability.clone().unwrap_or_default(),
        prefer_stable: manifest.prefer_stable,
    };

    // Try the lockfile fast path first. If composer.lock exists and its
    // content-hash matches the current composer.json's require sections,
    // skip the resolver entirely — just install the listed packages.
    let resolved_deps =
        if let Some(deps) = try_load_from_lockfile(project_dir, &manifest, !args.no_dev)? {
            if !args.quiet {
                println!(
                    "Using lockfile (skipped resolver) — {} packages",
                    deps.len()
                );
            }
            deps
        } else {
            // No valid lockfile — fall back to full resolver
            let registry = PackagistClient::new(&base_url);
            let resolver = Resolver::new(registry);

            if !args.quiet {
                println!("Resolving dependencies...");
            }

            resolver
                .resolve(manifest.require.clone(), manifest.require_dev.clone(), opts)
                .await
                .context("dependency resolution failed")?
        };

    if !args.quiet {
        println!("Resolved {} packages", resolved_deps.len());
    }

    // 3. Download + extract
    let vendor_dir = project_dir.join("vendor");
    let cache_dir = dirs_cache_dir();

    let installer = Installer::new(vendor_dir.clone(), cache_dir);

    if !args.quiet {
        println!("Installing packages...");
    }

    installer
        .install_all(&resolved_deps)
        .await
        .context("installation failed")?;

    // 4. Generate autoloader
    if !args.quiet {
        println!("Generating autoloader...");
    }

    let packages = scan_installed_packages(&vendor_dir);
    let spec = AutoloadSpec {
        vendor_dir: vendor_dir.clone(),
        root_autoload: serde_json::to_value(&manifest.autoload).unwrap_or_default(),
        packages,
    };
    AutoloadGenerator::generate(&spec).context("autoloader generation failed")?;

    // 5. Write composer.lock
    if !args.quiet {
        println!("Writing composer.lock...");
    }

    let lock = build_lock(&manifest, &resolved_deps, !args.no_dev);
    let lock_json = lock.serialize_to_string().context("serializing lock")?;
    std::fs::write(project_dir.join("composer.lock"), lock_json)
        .context("writing composer.lock")?;

    if !args.quiet {
        println!("✓ Install complete ({} packages)", resolved_deps.len());
    }

    Ok(())
}

/// Determine the content-addressed cache directory.
fn dirs_cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("TUSK_CACHE_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".cache/tusk")
    } else {
        PathBuf::from("/tmp/tusk-cache")
    }
}

/// Scan the vendor/ directory for installed packages and collect autoload info.
fn scan_installed_packages(vendor_dir: &Path) -> Vec<InstalledPackage> {
    let mut packages = Vec::new();

    // vendor/{vendor}/{package}/composer.json
    let Ok(vendors) = std::fs::read_dir(vendor_dir) else {
        return packages;
    };
    for vendor_entry in vendors.flatten() {
        let vendor_path = vendor_entry.path();
        if !vendor_path.is_dir() {
            continue;
        }
        let Ok(pkgs) = std::fs::read_dir(&vendor_path) else {
            continue;
        };
        for pkg_entry in pkgs.flatten() {
            let pkg_path = pkg_entry.path();
            let cj_path = pkg_path.join("composer.json");
            if cj_path.exists() {
                if let Ok(json) = std::fs::read_to_string(&cj_path) {
                    if let Ok(cj) = ComposerJson::from_str(&json) {
                        let name = pkg_path
                            .strip_prefix(vendor_dir)
                            .unwrap_or(&pkg_path)
                            .to_string_lossy()
                            .to_string();
                        packages.push(InstalledPackage {
                            name,
                            autoload: serde_json::to_value(&cj.autoload).unwrap_or_default(),
                        });
                    }
                }
            }
        }
    }

    packages
}

/// Build a `ComposerLock` from the resolved dependency set.
fn build_lock(
    manifest: &ComposerJson,
    resolved: &[tusk_resolver::ResolvedDependency],
    include_dev: bool,
) -> tusk_manifest::ComposerLock {
    use tusk_manifest::{ComposerLock, Dist, LockedPackage};

    let mut packages = Vec::new();
    let mut packages_dev = Vec::new();

    // Determine which packages are dev-only
    let dev_names: std::collections::HashSet<&String> = manifest.require_dev.keys().collect();

    for dep in resolved {
        let locked = LockedPackage {
            name: dep.name.clone(),
            version: dep.version.to_composer_string(),
            source: serde_json::Value::Null,
            dist: Dist {
                url: dep.dist.url.clone(),
                r#type: dep.dist.r#type.clone(),
                shasum: dep.dist.shasum.clone(),
                reference: None,
            },
            require: dep.require.clone(),
            require_dev: indexmap::IndexMap::new(),
            conflict: indexmap::IndexMap::new(),
            replace: indexmap::IndexMap::new(),
            provide: indexmap::IndexMap::new(),
            suggest: indexmap::IndexMap::new(),
            type_field: None,
            autoload: serde_json::Value::Null,
            autoload_dev: serde_json::Value::Null,
            license: Vec::new(),
            authors: Vec::new(),
            description: None,
            homepage: None,
            keywords: Vec::new(),
            time: None,
            notification_url: None,
        };

        if include_dev && dev_names.contains(&dep.name) {
            packages_dev.push(locked);
        } else {
            packages.push(locked);
        }
    }

    // Compute content hash (simple hash of composer.json require sections)
    let content_hash = compute_content_hash(manifest);

    ComposerLock {
        readme: None,
        content_hash: Some(content_hash),
        packages,
        packages_dev,
        aliases: Vec::new(),
        minimum_stability: manifest
            .minimum_stability
            .clone()
            .unwrap_or_else(|| "stable".to_string()),
        stability_flags: indexmap::IndexMap::new(),
        prefer_stable: manifest.prefer_stable,
        prefer_lowest: false,
        platform: extract_platform(&manifest.require),
        platform_dev: extract_platform(&manifest.require_dev),
        plugin_api_version: None,
    }
}

/// Extract platform requirements (php, ext-*) from a require map.
fn extract_platform(require: &tusk_manifest::RequireMap) -> indexmap::IndexMap<String, String> {
    let mut platform = indexmap::IndexMap::new();
    for (name, constraint) in require {
        if name == "php"
            || name.starts_with("ext-")
            || name.starts_with("lib-")
            || name.starts_with("composer-")
        {
            platform.insert(name.clone(), constraint.clone());
        }
    }
    platform
}

/// Compute a simple content hash for the lock file.
fn compute_content_hash(manifest: &ComposerJson) -> String {
    use sha1_smol::Sha1;
    let mut hasher = Sha1::new();
    // Hash the require + require-dev sections
    hasher.update(
        serde_json::to_string(&manifest.require)
            .unwrap_or_default()
            .as_bytes(),
    );
    hasher.update(
        serde_json::to_string(&manifest.require_dev)
            .unwrap_or_default()
            .as_bytes(),
    );
    hasher.digest().to_string()
}

/// Try to load resolved dependencies from a valid composer.lock.
///
/// Returns `Some(deps)` if the lockfile exists and its content-hash matches
/// the current composer.json's require sections (skips the resolver entirely).
/// Returns `None` if there's no lockfile, or the hash doesn't match (caller
/// must re-resolve and rewrite the lockfile).
///
/// This is the "frozen lockfile" fast path — Bun's headline cold-cache
/// optimization. For projects with a committed lockfile, cold install time
/// drops to the download phase only.
fn try_load_from_lockfile(
    project_dir: &Path,
    manifest: &ComposerJson,
    include_dev: bool,
) -> anyhow::Result<Option<Vec<tusk_resolver::ResolvedDependency>>> {
    use tusk_manifest::ComposerLock;

    let lock_path = project_dir.join("composer.lock");
    if !lock_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&lock_path)
        .map_err(|e| anyhow::anyhow!("reading composer.lock: {e}"))?;
    let lock = ComposerLock::deserialize_str(&content)
        .map_err(|e| anyhow::anyhow!("parsing composer.lock: {e}"))?;

    // Verify content-hash matches
    let current_hash = compute_content_hash(manifest);
    if lock.content_hash.as_deref() != Some(current_hash.as_str()) {
        // Hash mismatch — manifest changed, must re-resolve
        return Ok(None);
    }

    // Convert locked packages to ResolvedDependency
    let mut resolved = Vec::new();
    for pkg in &lock.packages {
        if let Some(dep) = locked_to_resolved(pkg) {
            resolved.push(dep);
        }
    }
    if include_dev {
        for pkg in &lock.packages_dev {
            if let Some(dep) = locked_to_resolved(pkg) {
                resolved.push(dep);
            }
        }
    }
    if resolved.is_empty() {
        return Ok(None);
    }
    Ok(Some(resolved))
}

/// Convert a `LockedPackage` into a `ResolvedDependency`.
///
/// Returns None if the version string can't be parsed (we'd need to re-resolve).
fn locked_to_resolved(
    pkg: &tusk_manifest::LockedPackage,
) -> Option<tusk_resolver::ResolvedDependency> {
    let version = tusk_semver::Version::parse(&pkg.version).ok()?;
    Some(tusk_resolver::ResolvedDependency {
        name: pkg.name.clone(),
        version,
        require: pkg.require.clone(),
        dist: tusk_registry::DistRef {
            url: pkg.dist.url.clone(),
            shasum: pkg.dist.shasum.clone(),
            r#type: pkg.dist.r#type.clone(),
        },
    })
}
