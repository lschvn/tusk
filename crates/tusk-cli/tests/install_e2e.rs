//! End-to-end integration test: `tusk install` in a temp project.
//!
//! Sets up a wiremock server to serve:
//! 1. Packagist p2 metadata endpoint
//! 2. A dist zip archive
//!
//! Then runs `tusk install` and asserts the full pipeline works:
//! composer.json → resolve → download+extract → autoload → lock.

#![allow(clippy::pedantic)]

use std::fs;
use std::io::Write;

use assert_cmd::Command;
use predicates::prelude::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Build a ZIP archive in memory containing a composer.json with PSR-4 autoload.
fn make_package_zip() -> (Vec<u8>, String) {
    let buf: Vec<u8> = Vec::new();
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(buf));
    let opts = zip::write::SimpleFileOptions::default();

    // composer.json with PSR-4 autoload
    zip.start_file("composer.json", opts).unwrap();
    zip.write_all(br#"{"name":"acme/foo","autoload":{"psr-4":{"Acme\\Foo\\":"src/"}}}"#)
        .unwrap();

    // A PHP source file
    let opts2 = zip::write::SimpleFileOptions::default();
    zip.start_file("src/Foo.php", opts2).unwrap();
    zip.write_all(b"<?php\nnamespace Acme\\Foo;\nclass Foo {}\n")
        .unwrap();

    let result = zip.finish().unwrap();
    let bytes = result.into_inner();

    // Compute sha1
    use sha1_smol::Sha1;
    let mut hasher = Sha1::new();
    hasher.update(&bytes);
    let hash = hasher.digest().to_string();
    (bytes, hash)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tusk_install_end_to_end() {
    let (zip_bytes, sha1) = make_package_zip();

    // Start mock server
    let server = MockServer::start().await;

    // Mock: p2 metadata endpoint
    let dist_url = format!("{}/acme/foo-1.0.0.zip", server.uri());
    let metadata_json = serde_json::json!({
        "packages": {
            "acme/foo": [
                {
                    "version": "1.0.0",
                    "dist": {
                        "url": dist_url,
                        "type": "zip",
                        "shasum": sha1,
                    },
                    "require": {}
                }
            ]
        }
    });

    Mock::given(method("GET"))
        .and(path("/p2/acme/foo.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(metadata_json))
        .mount(&server)
        .await;

    // Mock: dist archive download
    Mock::given(method("GET"))
        .and(path("/acme/foo-1.0.0.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(zip_bytes.clone(), "application/zip"))
        .mount(&server)
        .await;

    // Create a temp project
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path();

    // Write composer.json
    fs::write(
        project_dir.join("composer.json"),
        r#"{
    "name": "test/project",
    "require": {
        "acme/foo": "^1.0"
    }
}"#,
    )
    .unwrap();

    // Use a temp cache dir so it doesn't interfere with real installs
    let cache_dir = project_dir.join(".tusk-cache");

    // Run `tusk install`
    let mut cmd = Command::cargo_bin("tusk").unwrap();
    cmd.current_dir(project_dir)
        .env("TUSK_CACHE_DIR", cache_dir.to_str().unwrap())
        .arg("install")
        .arg("--quiet")
        .arg("--packagist-url")
        .arg(server.uri())
        .assert()
        .success();

    // Assert vendor/ structure
    let vendor = project_dir.join("vendor");
    assert!(vendor.exists(), "vendor/ must exist");

    let pkg_dir = vendor.join("acme/foo");
    assert!(pkg_dir.exists(), "vendor/acme/foo must exist");
    assert!(
        pkg_dir.join("composer.json").exists(),
        "extracted composer.json must exist"
    );
    assert!(
        pkg_dir.join("src/Foo.php").exists(),
        "extracted src/Foo.php must exist"
    );

    // Assert autoload.php
    let autoload_php = vendor.join("autoload.php");
    assert!(autoload_php.exists(), "vendor/autoload.php must exist");
    let autoload_content = fs::read_to_string(&autoload_php).unwrap();
    assert!(
        autoload_content.contains("spl_autoload_register"),
        "autoload.php must register an autoloader"
    );

    // Assert PSR-4 map
    let psr4_php = vendor.join("composer/autoload_psr4.php");
    assert!(psr4_php.exists(), "autoload_psr4.php must exist");
    let psr4_content = fs::read_to_string(&psr4_php).unwrap();
    assert!(
        psr4_content.contains("Acme"),
        "PSR-4 map must contain Acme namespace"
    );

    // Assert composer.lock
    let lock_path = project_dir.join("composer.lock");
    assert!(lock_path.exists(), "composer.lock must exist");
    let lock_content = fs::read_to_string(&lock_path).unwrap();
    let lock_json: serde_json::Value = serde_json::from_str(&lock_content).unwrap();
    assert!(
        lock_json["packages"].is_array(),
        "lock must have packages array"
    );
    let pkgs = lock_json["packages"].as_array().unwrap();
    assert!(
        pkgs.iter().any(|p| p["name"] == "acme/foo"),
        "lock must contain acme/foo"
    );
    assert_eq!(
        pkgs[0]["version"], "1.0.0",
        "lock must record version 1.0.0"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tusk_install_exits_nonzero_on_missing_composer_json() {
    let tmp = tempfile::tempdir().unwrap();

    Command::cargo_bin("tusk")
        .unwrap()
        .current_dir(tmp.path())
        .arg("install")
        .assert()
        .failure()
        .stderr(predicate::str::contains("composer.json"));
}
