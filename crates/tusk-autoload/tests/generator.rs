//! Tests for the autoloader generator.
//!
//! We create a fake vendor/ directory with packages that have PSR-4,
//! classmap, and files autoload sections, run the generator, and assert
//! the PHP output files exist and contain the right content.

#![allow(clippy::pedantic)]

use std::fs;
use tusk_autoload::{AutoloadGenerator, AutoloadSpec, InstalledPackage};
use tusk_manifest::ComposerJson;

/// Helper: write a minimal composer.json into a vendor package dir.
fn make_vendor_pkg(
    vendor_dir: &std::path::Path,
    name: &str,
    autoload_json: &str,
) -> InstalledPackage {
    let pkg_dir = vendor_dir.join(name);
    fs::create_dir_all(&pkg_dir).unwrap();
    let cj_json = format!(r#"{{"name":"{name}","autoload":{autoload_json}}}"#);
    fs::write(pkg_dir.join("composer.json"), &cj_json).unwrap();

    let cj = ComposerJson::from_str(&cj_json).unwrap();
    InstalledPackage {
        name: name.to_string(),
        autoload: serde_json::to_value(&cj.autoload).unwrap(),
    }
}

#[test]
fn generate_psr4_autoload() {
    let tmp = tempfile::tempdir().unwrap();
    let vendor_dir = tmp.path().join("vendor");
    fs::create_dir_all(&vendor_dir).unwrap();

    // Package with PSR-4: Acme\Foo => src/
    make_vendor_pkg(
        &vendor_dir,
        "acme/foo",
        r#"{"psr-4":{"Acme\\Foo\\":"src/"}}"#,
    );

    let spec = AutoloadSpec {
        vendor_dir: vendor_dir.clone(),
        root_autoload: serde_json::Value::Null,
        packages: spec_from_vendor(&vendor_dir),
    };

    AutoloadGenerator::generate(&spec).expect("generate should succeed");

    // Check autoload.php exists
    let autoload_php = vendor_dir.join("autoload.php");
    assert!(autoload_php.exists(), "vendor/autoload.php must exist");

    // Check PSR-4 map
    let psr4_php = vendor_dir.join("composer/autoload_psr4.php");
    assert!(psr4_php.exists(), "autoload_psr4.php must exist");
    let psr4_content = fs::read_to_string(&psr4_php).unwrap();
    assert!(
        psr4_content.contains("Acme\\Foo\\"),
        "PSR-4 map must contain the namespace, got:\n{psr4_content}"
    );

    // Check the path is relative to vendor dir
    assert!(
        psr4_content.contains("acme/foo/src"),
        "PSR-4 map must contain the package path, got:\n{psr4_content}"
    );
}

#[test]
fn generate_files_autoload() {
    let tmp = tempfile::tempdir().unwrap();
    let vendor_dir = tmp.path().join("vendor");
    fs::create_dir_all(&vendor_dir).unwrap();

    // Create the actual file so the generator can find it
    fs::create_dir_all(vendor_dir.join("acme/foo/src")).unwrap();
    fs::write(
        vendor_dir.join("acme/foo/src/bootstrap.php"),
        "<?php // bootstrap",
    )
    .unwrap();

    make_vendor_pkg(
        &vendor_dir,
        "acme/foo",
        r#"{"psr-4":{"Acme\\Foo\\":"src/"},"files":["src/bootstrap.php"]}"#,
    );

    let spec = AutoloadSpec {
        vendor_dir: vendor_dir.clone(),
        root_autoload: serde_json::Value::Null,
        packages: spec_from_vendor(&vendor_dir),
    };

    AutoloadGenerator::generate(&spec).expect("generate should succeed");

    let files_php = vendor_dir.join("composer/autoload_files.php");
    assert!(files_php.exists(), "autoload_files.php must exist");
    let content = fs::read_to_string(&files_php).unwrap();
    assert!(
        content.contains("bootstrap.php"),
        "files map must contain bootstrap.php, got:\n{content}"
    );
}

#[test]
fn generate_deterministic_output() {
    let tmp = tempfile::tempdir().unwrap();
    let vendor_dir = tmp.path().join("vendor");
    fs::create_dir_all(&vendor_dir).unwrap();

    // Create two packages so ordering matters
    make_vendor_pkg(
        &vendor_dir,
        "zzz/last",
        r#"{"psr-4":{"Zzz\\Last\\":"src/"}}"#,
    );
    make_vendor_pkg(
        &vendor_dir,
        "aaa/first",
        r#"{"psr-4":{"Aaa\\First\\":"src/"}}"#,
    );

    let spec = AutoloadSpec {
        vendor_dir: vendor_dir.clone(),
        root_autoload: serde_json::Value::Null,
        packages: spec_from_vendor(&vendor_dir),
    };

    // Generate twice into different dirs
    let dir1 = tmp.path().join("v1");
    let dir2 = tmp.path().join("v2");
    let spec1 = AutoloadSpec {
        vendor_dir: dir1.clone(),
        root_autoload: serde_json::Value::Null,
        packages: spec.packages.clone(),
    };
    let spec2 = AutoloadSpec {
        vendor_dir: dir2.clone(),
        root_autoload: serde_json::Value::Null,
        packages: spec.packages.clone(),
    };

    AutoloadGenerator::generate(&spec1).unwrap();
    AutoloadGenerator::generate(&spec2).unwrap();

    let psr4_1 = fs::read_to_string(dir1.join("composer/autoload_psr4.php")).unwrap();
    let psr4_2 = fs::read_to_string(dir2.join("composer/autoload_psr4.php")).unwrap();
    // Paths differ by dir prefix, so compare relative content
    let rel1 = psr4_1.replace("/v1/", "/VENDOR/");
    let rel2 = psr4_2.replace("/v2/", "/VENDOR/");
    assert_eq!(
        rel1, rel2,
        "PSR-4 map must be deterministic (same ordering) regardless of path"
    );
}

#[test]
fn autoload_php_is_valid_php() {
    let tmp = tempfile::tempdir().unwrap();
    let vendor_dir = tmp.path().join("vendor");
    fs::create_dir_all(&vendor_dir).unwrap();

    make_vendor_pkg(
        &vendor_dir,
        "acme/foo",
        r#"{"psr-4":{"Acme\\Foo\\":"src/"}}"#,
    );

    let spec = AutoloadSpec {
        vendor_dir: vendor_dir.clone(),
        root_autoload: serde_json::Value::Null,
        packages: spec_from_vendor(&vendor_dir),
    };

    AutoloadGenerator::generate(&spec).unwrap();

    let content = fs::read_to_string(vendor_dir.join("autoload.php")).unwrap();
    assert!(
        content.starts_with("<?php"),
        "autoload.php must start with <?php"
    );
    assert!(
        content.contains("spl_autoload_register"),
        "autoload.php must register an autoloader"
    );
}

/// Scan vendor/ for installed packages (two-level: vendor/{vendor}/{package}/).
fn spec_from_vendor(vendor_dir: &std::path::Path) -> Vec<InstalledPackage> {
    let mut packages = Vec::new();
    // vendor/{vendor}/{package}/composer.json
    if let Ok(vendors) = fs::read_dir(vendor_dir) {
        for vendor_entry in vendors.flatten() {
            let vendor_path = vendor_entry.path();
            if !vendor_path.is_dir() {
                continue;
            }
            if let Ok(pkgs) = fs::read_dir(&vendor_path) {
                for pkg_entry in pkgs.flatten() {
                    let pkg_path = pkg_entry.path();
                    let cj_path = pkg_path.join("composer.json");
                    if cj_path.exists() {
                        let json = fs::read_to_string(&cj_path).unwrap();
                        if let Ok(cj) = ComposerJson::from_str(&json) {
                            let name = pkg_path
                                .strip_prefix(vendor_dir)
                                .unwrap()
                                .to_string_lossy()
                                .to_string();
                            packages.push(InstalledPackage {
                                name,
                                autoload: serde_json::to_value(&cj.autoload).unwrap(),
                            });
                        }
                    }
                }
            }
        }
    }
    packages
}
