//! Tests for the installer: download, verify, extract, cache.

#![allow(clippy::pedantic)]

use tusk_installer::Installer;
use tusk_registry::{DistRef, MockRegistry, PackageMetadata, PackageVersion};
use tusk_resolver::{ResolveOptions, Resolver};
use tusk_semver::{Stability, Version};
use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};

fn v(major: u32, minor: u32, patch: u32) -> Version {
    Version {
        major,
        minor,
        patch,
        tweak: None,
        stability: Stability::Stable,
        stability_n: None,
        dev_branch: None,
        is_v_prefixed: false,
    }
}

/// Build a minimal valid ZIP archive in memory and return (bytes, sha1).
fn make_test_zip() -> (Vec<u8>, String) {
    use std::io::Write as _;
    // Create a simple zip with one file: composer.json
    let buf: Vec<u8> = Vec::new();
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(buf));
    let opts = zip::write::SimpleFileOptions::default();
    zip.start_file("composer.json", opts).unwrap();
    zip.write_all(b"{\"name\": \"acme/foo\"}").unwrap();
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
async fn installer_downloads_and_extracts_package() {
    let (zip_bytes, sha1) = make_test_zip();

    let server = MockServer::start().await;
    Mock::given(path("/acme/foo-1.0.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(zip_bytes.clone(), "application/zip"))
        .mount(&server)
        .await;

    let url = format!("{}/acme/foo-1.0.zip", server.uri());

    let registry = MockRegistry::new().with_package(
        "acme/foo",
        PackageMetadata {
            versions: vec![PackageVersion {
                version: v(1, 0, 0),
                dist: DistRef {
                    url,
                    shasum: sha1,
                    r#type: "zip".to_string(),
                },
                require: Default::default(),
            }],
        },
    );

    let tmp = tempfile::tempdir().unwrap();
    let cache_dir = tmp.path().join("cache");
    let vendor_dir = tmp.path().join("vendor");

    let resolver = Resolver::new(registry);
    let resolved = resolver
        .resolve(
            {
                let mut m = tusk_manifest::RequireMap::new();
                m.insert("acme/foo".to_string(), "^1.0".to_string());
                m
            },
            Default::default(),
            ResolveOptions::default(),
        )
        .await
        .expect("resolve");

    let installer = Installer::new(vendor_dir.clone(), cache_dir);
    installer
        .install_all(&resolved)
        .await
        .expect("install should succeed");

    // The package should be extracted to vendor/acme/foo/
    let pkg_dir = vendor_dir.join("acme/foo");
    assert!(pkg_dir.exists(), "vendor/acme/foo must exist");
    let composer_json = pkg_dir.join("composer.json");
    assert!(composer_json.exists(), "extracted composer.json must exist");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn installer_rejects_tampered_archive() {
    let (zip_bytes, real_sha1) = make_test_zip();

    let server = MockServer::start().await;
    // Serve a tampered version (truncated)
    let tampered = &zip_bytes[..zip_bytes.len() / 2];
    Mock::given(path("/acme/foo-1.0.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(tampered.to_vec(), "application/zip"))
        .mount(&server)
        .await;

    let url = format!("{}/acme/foo-1.0.zip", server.uri());

    let registry = MockRegistry::new().with_package(
        "acme/foo",
        PackageMetadata {
            versions: vec![PackageVersion {
                version: v(1, 0, 0),
                dist: DistRef {
                    url,
                    shasum: real_sha1, // expects the original sha1
                    r#type: "zip".to_string(),
                },
                require: Default::default(),
            }],
        },
    );

    let tmp = tempfile::tempdir().unwrap();
    let resolver = Resolver::new(registry);
    let resolved = resolver
        .resolve(
            {
                let mut m = tusk_manifest::RequireMap::new();
                m.insert("acme/foo".to_string(), "^1.0".to_string());
                m
            },
            Default::default(),
            ResolveOptions::default(),
        )
        .await
        .expect("resolve");

    let installer = Installer::new(tmp.path().join("vendor"), tmp.path().join("cache"));
    let result = installer.install_all(&resolved).await;

    assert!(
        result.is_err(),
        "tampered archive must be rejected (shasum mismatch)"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cache_hit_skips_download() {
    let (zip_bytes, sha1) = make_test_zip();

    let server = MockServer::start().await;
    Mock::given(path("/acme/foo-1.0.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(zip_bytes.clone(), "application/zip"))
        .expect(1) // should only be called ONCE
        .mount(&server)
        .await;

    let url = format!("{}/acme/foo-1.0.zip", server.uri());

    let tmp = tempfile::tempdir().unwrap();
    let cache_dir = tmp.path().join("cache");
    let vendor1 = tmp.path().join("vendor1");
    let vendor2 = tmp.path().join("vendor2");

    let resolved = vec![tusk_resolver::ResolvedDependency {
        name: "acme/foo".to_string(),
        version: v(1, 0, 0),
        require: Default::default(),
        dist: DistRef {
            url: url.clone(),
            shasum: sha1.clone(),
            r#type: "zip".to_string(),
        },
    }];

    // First install: downloads from network
    let installer1 = Installer::new(vendor1, cache_dir.clone());
    installer1
        .install_all(&resolved)
        .await
        .expect("first install");

    // Second install: should use cache, not hit network
    let installer2 = Installer::new(vendor2, cache_dir);
    installer2
        .install_all(&resolved)
        .await
        .expect("second install (cached)");

    // If the mock got called twice, expect(1) would fail verification
    server.verify().await;
}
