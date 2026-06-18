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

/// Verify the lockfile fast path: if composer.lock exists and its
/// content-hash matches composer.json, the CLI should skip the resolver
/// and use the lockfile's resolved packages directly.
///
/// **Unambiguous design:**
///
/// 1. Start a mock server that serves both `/p2/acme/foo.json` (metadata)
///    and the zip archive.
/// 2. Run a first install — this creates `composer.lock` whose
///    `dist.url` points at this same server.
/// 3. `server.reset()` and re-mount only the zip endpoint. Mount a
///    `/p2/` mock that **rejects** with HTTP 500.
/// 4. Wipe the **entire** archive cache (`TUSK_CACHE_DIR`) so the
///    second install must re-download from the lockfile URL.
/// 5. Run a second install.
///    - **Fast path works** → no resolver call, no `/p2/` request,
///      zip downloads from the (still-served) lockfile URL → success.
///    - **Fast path broken** → resolver hits `/p2/`, gets HTTP 500,
///      install fails with a network/resolve error.
///
/// This test also records the `content-hash` value, the lockfile's
/// `dist.url`, and timings for both installs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tusk_install_uses_lockfile_fast_path() {
    use std::time::Instant;

    // Same fixture as the end-to-end test
    let (zip_bytes, sha1) = make_package_zip();
    let server = wiremock::MockServer::start().await;

    // --- Mock setup for the FIRST install ---
    let dist_url = format!("{}/acme/foo-1.0.0.zip", server.uri());
    let metadata_json = serde_json::json!({
        "packages": {
            "acme/foo": [{
                "version": "1.0.0",
                "dist": { "url": dist_url.clone(), "shasum": sha1, "type": "zip" },
                "require": {}
            }]
        }
    });

    // Metadata endpoint — `expect(1)` means it must be called exactly once
    // (the first install). After that, the mock is "satisfied" and further
    // calls would 404, which is what we want for the second install.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/p2/acme/foo.json"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(metadata_json))
        .expect(1)
        .mount(&server)
        .await;

    // Zip endpoint — no `expect`, always available.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/acme/foo-1.0.0.zip"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(zip_bytes.clone(), "application/zip"),
        )
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path();
    std::fs::write(
        project_dir.join("composer.json"),
        r#"{
    "name": "test/project",
    "require": { "acme/foo": "^1.0" }
}"#,
    )
    .unwrap();
    let cache_dir = project_dir.join(".tusk-cache");
    std::fs::create_dir_all(&cache_dir).unwrap();

    // === First install (resolver path) ===
    let t1 = Instant::now();
    assert_cmd::Command::cargo_bin("tusk")
        .expect("cargo_bin should resolve")
        .current_dir(project_dir)
        .env("TUSK_CACHE_DIR", cache_dir.to_str().unwrap())
        .arg("install")
        .arg("--packagist-url")
        .arg(server.uri())
        .assert()
        .success();
    let first_elapsed = t1.elapsed();

    let lock_path = project_dir.join("composer.lock");
    assert!(lock_path.exists(), "composer.lock must be created");

    // Read the content-hash and the dist URL from the lockfile.
    let lock_content = std::fs::read_to_string(&lock_path).unwrap();
    let lock_json: serde_json::Value = serde_json::from_str(&lock_content).unwrap();
    let stored_content_hash = lock_json["content-hash"]
        .as_str()
        .expect("content-hash must be present in lockfile")
        .to_string();
    let lockfile_dist_url = lock_json["packages"][0]["dist"]["url"]
        .as_str()
        .expect("packages[0].dist.url must be present")
        .to_string();
    eprintln!("[diag] content-hash = {stored_content_hash}");
    eprintln!("[diag] lockfile dist.url = {lockfile_dist_url}");
    eprintln!("[diag] first install elapsed = {first_elapsed:?}");

    // === Prepare for the second install ===
    // Wipe the entire archive cache so the second install must re-download.
    std::fs::remove_dir_all(&cache_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    // Reset the server: all mocks are gone. Re-mount only the zip
    // endpoint, and mount a /p2/ mock that REJECTS with HTTP 500.
    server.reset().await;

    // Zip endpoint — still served, so the lockfile URL works.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/acme/foo-1.0.0.zip"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_raw(zip_bytes.clone(), "application/zip"),
        )
        .mount(&server)
        .await;

    // /p2/ endpoint — REJECTS with 500. If the resolver runs, it will
    // hit this and fail. The fast path skips the resolver entirely.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/p2/acme/foo.json"))
        .respond_with(wiremock::ResponseTemplate::new(500))
        .expect(0) // must NOT be called — proves fast path
        .named("p2-reject-must-not-be-called")
        .mount(&server)
        .await;

    // === Second install (fast path) ===
    let t2 = Instant::now();
    let result = assert_cmd::Command::cargo_bin("tusk")
        .expect("cargo_bin should resolve")
        .current_dir(project_dir)
        .env("TUSK_CACHE_DIR", cache_dir.to_str().unwrap())
        .arg("install")
        .arg("--packagist-url")
        .arg(server.uri())
        .assert()
        .success(); // fast path → no resolver → no /p2/ → install succeeds
    let second_elapsed = t2.elapsed();

    let output = result.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // The CLI should report it used the lockfile.
    assert!(
        stdout.contains("Using lockfile") || stderr.contains("Using lockfile"),
        "expected 'Using lockfile' message, got stdout={stdout:?}, stderr={stderr:?}"
    );

    // Wiremock verification: p2-reject must NOT have been called.
    // This is the definitive proof: if the resolver ran, it would have
    // hit /p2/ and gotten a 500, failing the install. The fact that
    // the install succeeded means the resolver never ran.
    server.verify().await; // panics if p2-reject.expect(0) was violated

    eprintln!("[diag] second install elapsed = {second_elapsed:?}");
    let speedup = first_elapsed.as_secs_f64() / second_elapsed.as_secs_f64().max(0.001);
    eprintln!("[diag] speedup = {speedup:.2}x");

    // Sanity: the install actually produced a vendor/ directory.
    let vendor = project_dir.join("vendor");
    assert!(vendor.join("acme/foo/composer.json").exists());

    // Suppress unused warning for the named mock reference.
}
