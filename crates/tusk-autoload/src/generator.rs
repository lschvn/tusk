//! Autoloader generator: collects autoload sections from installed packages
//! and generates Composer-compatible PHP files in `vendor/`.

#![allow(clippy::all)]

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// Error during autoloader generation.
#[derive(Debug, Error)]
pub enum AutoloadError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("autoload parse error in {package}: {detail}")]
    Parse { package: String, detail: String },
}

/// One installed package's autoload information.
#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub name: String,
    pub autoload: serde_json::Value,
}

/// Specification for autoloader generation.
#[derive(Debug, Clone)]
pub struct AutoloadSpec {
    /// Path to the `vendor/` directory.
    pub vendor_dir: PathBuf,
    /// Root project's own autoload section (may be `Null`).
    pub root_autoload: serde_json::Value,
    /// Each installed package's autoload info.
    pub packages: Vec<InstalledPackage>,
}

/// Generates `vendor/autoload.php` + `vendor/composer/autoload_*.php`.
pub struct AutoloadGenerator;

impl AutoloadGenerator {
    /// Generate all autoloader files into `spec.vendor_dir`.
    pub fn generate(spec: &AutoloadSpec) -> Result<(), AutoloadError> {
        let vendor_dir = &spec.vendor_dir;
        let composer_dir = vendor_dir.join("composer");
        std::fs::create_dir_all(&composer_dir)?;

        // Collect PSR-4, classmap, and files from all packages + root.
        let mut psr4_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut classmap: BTreeMap<String, String> = BTreeMap::new();
        let mut files_list: Vec<String> = Vec::new();

        // Root project autoload (paths relative to vendor/../ = base dir).
        let base_dir = vendor_dir.parent().unwrap_or(vendor_dir);
        collect_autoload(
            &spec.root_autoload,
            base_dir,
            &mut psr4_map,
            &mut classmap,
            &mut files_list,
        );

        // Each installed package.
        for pkg in &spec.packages {
            let pkg_dir = vendor_dir.join(&pkg.name);
            collect_autoload(
                &pkg.autoload,
                &pkg_dir,
                &mut psr4_map,
                &mut classmap,
                &mut files_list,
            );
        }

        // Generate PHP files.
        write_psr4_php(&composer_dir, &psr4_map)?;
        write_classmap_php(&composer_dir, &classmap)?;
        write_files_php(&composer_dir, &files_list)?;
        write_autoload_php(vendor_dir)?;

        Ok(())
    }
}

/// Parse an autoload JSON value and collect PSR-4, classmap, files entries.
fn collect_autoload(
    autoload: &serde_json::Value,
    base_dir: &Path,
    psr4_map: &mut BTreeMap<String, Vec<String>>,
    classmap: &mut BTreeMap<String, String>,
    files_list: &mut Vec<String>,
) {
    let Some(obj) = autoload.as_object() else {
        return; // no autoload section
    };

    // PSR-4: { "psr-4": { "Acme\\Foo\\": "src/" } }
    if let Some(psr4) = obj.get("psr-4").and_then(|v| v.as_object()) {
        for (prefix, paths) in psr4 {
            let path_strs: Vec<String> = match paths {
                serde_json::Value::String(s) => vec![s.clone()],
                serde_json::Value::Array(arr) => arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect(),
                _ => vec![],
            };
            let resolved: Vec<String> = path_strs
                .iter()
                .map(|p| resolve_path(base_dir, p))
                .collect();
            psr4_map.entry(prefix.clone()).or_default().extend(resolved);
        }
    }

    // Classmap: { "classmap": ["src/Class1.php", "src/Class2.php"] }
    if let Some(cm) = obj.get("classmap").and_then(|v| v.as_array()) {
        for entry in cm {
            if let Some(path_str) = entry.as_str() {
                let full = resolve_path(base_dir, path_str);
                // If it's a directory, scan it for .php files and map ClassName => file
                // For now, just store the path — full class scanning comes later.
                let path = std::path::Path::new(&full);
                if path.is_dir() {
                    scan_dir_for_classes(&full, classmap).ok();
                } else if path.is_file() {
                    // Try to infer class name from filename
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        classmap.insert(stem.to_string(), full.clone());
                    }
                }
            }
        }
    }

    // Files: { "files": ["src/bootstrap.php"] }
    if let Some(fl) = obj.get("files").and_then(|v| v.as_array()) {
        for entry in fl {
            if let Some(path_str) = entry.as_str() {
                files_list.push(resolve_path(base_dir, path_str));
            }
        }
    }

    // PSR-0: treat like PSR-4 for basic cases (legacy).
    if let Some(psr0) = obj.get("psr-0").and_then(|v| v.as_object()) {
        for (prefix, paths) in psr0 {
            let path_strs: Vec<String> = match paths {
                serde_json::Value::String(s) => vec![s.clone()],
                serde_json::Value::Array(arr) => arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect(),
                _ => vec![],
            };
            let resolved: Vec<String> = path_strs
                .iter()
                .map(|p| resolve_path(base_dir, p))
                .collect();
            // PSR-0 uses the underscore-style namespace, but we map it the same way.
            psr4_map.entry(prefix.clone()).or_default().extend(resolved);
        }
    }
}

/// Resolve a relative autoload path against a base directory.
/// Trims trailing slashes and normalizes.
fn resolve_path(base_dir: &Path, relative: &str) -> String {
    let trimmed = relative.trim_end_matches('/');
    let joined = base_dir.join(trimmed);
    joined.to_string_lossy().to_string()
}

/// Recursively scan a directory for PHP files and map `ClassName` => path.
/// Extracts the class name from `class Foo` or `namespace X; class Y` patterns.
fn scan_dir_for_classes(dir: &str, classmap: &mut BTreeMap<String, String>) -> std::io::Result<()> {
    for entry in walk_dir(dir)? {
        if std::path::Path::new(&entry)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("php"))
        {
            if let Ok(content) = std::fs::read_to_string(&entry) {
                if let Some(class_name) = extract_class_name(&content) {
                    classmap.insert(class_name, entry);
                }
            }
        }
    }
    Ok(())
}

/// Walk a directory recursively, returning all file paths.
fn walk_dir(dir: &str) -> std::io::Result<Vec<String>> {
    let mut results = Vec::new();
    walk_dir_inner(dir, &mut results)?;
    Ok(results)
}

fn walk_dir_inner(dir: &str, results: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_dir_inner(&path.to_string_lossy(), results)?;
        } else if path.is_file() {
            results.push(path.to_string_lossy().to_string());
        }
    }
    Ok(())
}

/// Extract the fully-qualified class name from PHP source.
/// Looks for `namespace X\Y;` and `class Foo`.
fn extract_class_name(php: &str) -> Option<String> {
    let namespace = php.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with("namespace ") {
            let ns = trimmed
                .trim_start_matches("namespace ")
                .trim_end_matches(';')
                .trim();
            Some(ns.replace('\\', "\\\\"))
        } else {
            None
        }
    });
    let class = php.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with("class ")
            || trimmed.starts_with("abstract class ")
            || trimmed.starts_with("final class ")
        {
            let after = trimmed
                .trim_start_matches("abstract ")
                .trim_start_matches("final ")
                .trim_start_matches("class ");
            let name = after.split_whitespace().next()?;
            Some(name.to_string())
        } else {
            None
        }
    });
    match (namespace, class) {
        (Some(ns), Some(cls)) => Some(format!("{ns}\\\\{cls}")),
        (None, Some(cls)) => Some(cls),
        _ => None,
    }
}

// --- PHP file writers ---

fn write_psr4_php(
    composer_dir: &Path,
    psr4_map: &BTreeMap<String, Vec<String>>,
) -> Result<(), AutoloadError> {
    let mut php =
        String::from("<?php\n\n// autoload_psr4.php @generated by Tusk\n\nreturn array(\n");
    for (prefix, paths) in psr4_map {
        let paths_str: Vec<String> = paths.iter().map(|p| format!("'{p}'")).collect();
        let escaped_prefix = prefix.replace('\'', "\\'");
        let _ = write!(
            php,
            "  '{}' => array({}),\n",
            escaped_prefix,
            paths_str.join(", ")
        );
    }
    php.push_str(");\n");

    std::fs::write(composer_dir.join("autoload_psr4.php"), php)?;
    Ok(())
}

fn write_classmap_php(
    composer_dir: &Path,
    classmap: &BTreeMap<String, String>,
) -> Result<(), AutoloadError> {
    let mut php =
        String::from("<?php\n\n// autoload_classmap.php @generated by Tusk\n\nreturn array(\n");
    for (class, path) in classmap {
        let escaped_class = class.replace('\'', "\\'");
        let escaped_path = path.replace('\'', "\\'");
        let _ = write!(php, "  '{escaped_class}' => '{escaped_path}',\n");
    }
    php.push_str(");\n");

    std::fs::write(composer_dir.join("autoload_classmap.php"), php)?;
    Ok(())
}

fn write_files_php(composer_dir: &Path, files_list: &[String]) -> Result<(), AutoloadError> {
    let mut php =
        String::from("<?php\n\n// autoload_files.php @generated by Tusk\n\nreturn array(\n");
    for file in files_list {
        let escaped = file.replace('\'', "\\'");
        let _ = write!(php, "  '{escaped}',\n");
    }
    php.push_str(");\n");

    std::fs::write(composer_dir.join("autoload_files.php"), php)?;
    Ok(())
}

fn write_autoload_php(vendor_dir: &Path) -> Result<(), AutoloadError> {
    let php = r"<?php
// autoload.php @generated by Tusk

$tuskPsr4 = require __DIR__ . '/composer/autoload_psr4.php';
$tuskClassmap = require __DIR__ . '/composer/autoload_classmap.php';
$tuskFiles = require __DIR__ . '/composer/autoload_files.php';

spl_autoload_register(function (string $class) use (&$tuskPsr4, &$tuskClassmap): void {
    // PSR-4 lookup
    $prefix = $class;
    while (false !== $pos = strrpos($prefix, '\\')) {
        $prefix = substr($class, 0, $pos + 1);
        $relativeClass = substr($class, $pos + 1);
        if (isset($tuskPsr4[$prefix])) {
            foreach ($tuskPsr4[$prefix] as $baseDir) {
                $file = $baseDir . DIRECTORY_SEPARATOR . str_replace('\\', '/', $relativeClass) . '.php';
                if (is_file($file)) {
                    require $file;
                    return;
                }
            }
        }
        $prefix = rtrim($prefix, '\\');
    }

    // Classmap fallback
    if (isset($tuskClassmap[$class])) {
        require $tuskClassmap[$class];
    }
});

// Eager-require files
foreach ($tuskFiles as $file) {
    require_once $file;
}

return true;
";

    std::fs::write(vendor_dir.join("autoload.php"), php)?;
    Ok(())
}
