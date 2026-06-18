//! Tests for the dependency resolver.
//!
//! The resolver picks the highest version matching each constraint, then
//! recursively resolves transitive dependencies. When two packages require
//! incompatible versions of the same dependency, it produces a clear error.

#![allow(clippy::pedantic)]

use tusk_manifest::RequireMap;
use tusk_registry::{DistRef, MockRegistry, PackageMetadata, PackageVersion};
use tusk_resolver::{ResolveOptions, Resolver};
use tusk_semver::{Stability, Version};

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

fn pkg_version(ver: Version, url: &str, shasum: &str, require: &[(&str, &str)]) -> PackageVersion {
    let mut req = RequireMap::new();
    for (k, v) in require {
        req.insert((*k).to_string(), (*v).to_string());
    }
    PackageVersion {
        version: ver,
        dist: DistRef {
            url: url.to_string(),
            shasum: shasum.to_string(),
            r#type: "zip".to_string(),
        },
        require: req,
    }
}

fn metadata(versions: Vec<PackageVersion>) -> PackageMetadata {
    PackageMetadata { versions }
}

fn requires(pairs: &[(&str, &str)]) -> RequireMap {
    let mut m = RequireMap::new();
    for (k, v) in pairs {
        m.insert((*k).to_string(), (*v).to_string());
    }
    m
}

// ---------- basic resolution ----------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resolves_single_package_picks_highest_matching() {
    let registry = MockRegistry::new().with_package(
        "acme/foo",
        metadata(vec![
            pkg_version(v(1, 0, 0), "url-1.0", "s1", &[]),
            pkg_version(v(1, 1, 0), "url-1.1", "s2", &[]),
            pkg_version(v(1, 2, 0), "url-1.2", "s3", &[]),
        ]),
    );

    let resolver = Resolver::new(registry);
    let resolved = resolver
        .resolve(
            requires(&[("acme/foo", "^1.0")]),
            RequireMap::new(),
            ResolveOptions::default(),
        )
        .await
        .expect("should resolve");

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].name, "acme/foo");
    assert_eq!(resolved[0].version, v(1, 2, 0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resolves_transitive_dependencies() {
    // acme/app requires acme/lib ^1.0
    // acme/lib 1.2.0 requires acme/util ^2.0
    // acme/util 2.1.0 has no deps
    let registry = MockRegistry::new()
        .with_package(
            "acme/app",
            metadata(vec![pkg_version(
                v(1, 0, 0),
                "app-1.0",
                "sa",
                &[("acme/lib", "^1.0")],
            )]),
        )
        .with_package(
            "acme/lib",
            metadata(vec![pkg_version(
                v(1, 2, 0),
                "lib-1.2",
                "sl",
                &[("acme/util", "^2.0")],
            )]),
        )
        .with_package(
            "acme/util",
            metadata(vec![pkg_version(v(2, 1, 0), "util-2.1", "su", &[])]),
        );

    let resolver = Resolver::new(registry);
    let resolved = resolver
        .resolve(
            requires(&[("acme/app", "^1.0")]),
            RequireMap::new(),
            ResolveOptions::default(),
        )
        .await
        .expect("should resolve transitively");

    // All three packages should be resolved.
    let names: Vec<&str> = resolved.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"acme/app"));
    assert!(names.contains(&"acme/lib"));
    assert!(names.contains(&"acme/util"));
    assert_eq!(resolved.len(), 3, "exactly 3 packages should be resolved");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn picks_version_satisfying_all_constraints() {
    // acme/a requires acme/shared ^1.0
    // acme/b requires acme/shared >=1.2 <1.4
    // acme/shared has 1.1, 1.2, 1.3, 1.4
    // The intersection is >=1.2, <1.4 → highest = 1.3
    let registry = MockRegistry::new()
        .with_package(
            "acme/a",
            metadata(vec![pkg_version(
                v(1, 0, 0),
                "a",
                "sa",
                &[("acme/shared", "^1.0")],
            )]),
        )
        .with_package(
            "acme/b",
            metadata(vec![pkg_version(
                v(1, 0, 0),
                "b",
                "sb",
                &[("acme/shared", ">=1.2 <1.4")],
            )]),
        )
        .with_package(
            "acme/shared",
            metadata(vec![
                pkg_version(v(1, 1, 0), "s1", "s1", &[]),
                pkg_version(v(1, 2, 0), "s2", "s2", &[]),
                pkg_version(v(1, 3, 0), "s3", "s3", &[]),
                pkg_version(v(1, 4, 0), "s4", "s4", &[]),
            ]),
        );

    let resolver = Resolver::new(registry);
    let resolved = resolver
        .resolve(
            requires(&[("acme/a", "^1.0"), ("acme/b", "^1.0")]),
            RequireMap::new(),
            ResolveOptions::default(),
        )
        .await
        .expect("should resolve");

    let shared = resolved
        .iter()
        .find(|d| d.name == "acme/shared")
        .expect("acme/shared must be resolved");
    assert_eq!(
        shared.version,
        v(1, 3, 0),
        "should pick 1.3.0 (highest satisfying both constraints)"
    );
}

// ---------- conflicts ----------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conflict_produces_error_naming_packages() {
    // acme/x requires acme/core ^1.0
    // acme/y requires acme/core ^2.0
    // No single version of acme/core can satisfy both ^1.0 and ^2.0.
    let registry = MockRegistry::new()
        .with_package(
            "acme/x",
            metadata(vec![pkg_version(
                v(1, 0, 0),
                "x",
                "sx",
                &[("acme/core", "^1.0")],
            )]),
        )
        .with_package(
            "acme/y",
            metadata(vec![pkg_version(
                v(1, 0, 0),
                "y",
                "sy",
                &[("acme/core", "^2.0")],
            )]),
        )
        .with_package(
            "acme/core",
            metadata(vec![
                pkg_version(v(1, 5, 0), "c1", "sc1", &[]),
                pkg_version(v(2, 0, 0), "c2", "sc2", &[]),
            ]),
        );

    let resolver = Resolver::new(registry);
    let err = resolver
        .resolve(
            requires(&[("acme/x", "^1.0"), ("acme/y", "^1.0")]),
            RequireMap::new(),
            ResolveOptions::default(),
        )
        .await
        .expect_err("conflicting constraints should fail");

    let msg = err.to_string();
    assert!(
        msg.contains("acme/core"),
        "error must name the conflicting package: {msg}"
    );
}

// ---------- dev dependencies ----------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dev_dependencies_excluded_with_no_dev() {
    let registry = MockRegistry::new()
        .with_package(
            "acme/app",
            metadata(vec![pkg_version(v(1, 0, 0), "app", "sa", &[])]),
        )
        .with_package(
            "phpunit/phpunit",
            metadata(vec![pkg_version(v(10, 0, 0), "pu", "sp", &[])]),
        );

    let resolver = Resolver::new(registry);
    let opts = ResolveOptions {
        include_dev: false,
        ..Default::default()
    };
    let resolved = resolver
        .resolve(
            requires(&[("acme/app", "^1.0")]),
            requires(&[("phpunit/phpunit", "^10.0")]),
            opts,
        )
        .await
        .expect("should resolve");

    let names: Vec<&str> = resolved.iter().map(|d| d.name.as_str()).collect();
    assert!(
        !names.contains(&"phpunit/phpunit"),
        "dev dependency must be excluded with --no-dev"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dev_dependencies_included_by_default() {
    let registry = MockRegistry::new()
        .with_package(
            "acme/app",
            metadata(vec![pkg_version(v(1, 0, 0), "app", "sa", &[])]),
        )
        .with_package(
            "phpunit/phpunit",
            metadata(vec![pkg_version(v(10, 0, 0), "pu", "sp", &[])]),
        );

    let resolver = Resolver::new(registry);
    let opts = ResolveOptions {
        include_dev: true,
        ..Default::default()
    };
    let resolved = resolver
        .resolve(
            requires(&[("acme/app", "^1.0")]),
            requires(&[("phpunit/phpunit", "^10.0")]),
            opts,
        )
        .await
        .expect("should resolve");

    let names: Vec<&str> = resolved.iter().map(|d| d.name.as_str()).collect();
    assert!(
        names.contains(&"phpunit/phpunit"),
        "dev dependency must be included by default"
    );
}

// ---------- determinism ----------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn identical_inputs_produce_identical_resolution() {
    let registry = MockRegistry::new()
        .with_package(
            "acme/app",
            metadata(vec![pkg_version(
                v(1, 0, 0),
                "app",
                "sa",
                &[("acme/lib", "^1.0")],
            )]),
        )
        .with_package(
            "acme/lib",
            metadata(vec![
                pkg_version(v(1, 0, 0), "l1", "s1", &[]),
                pkg_version(v(1, 2, 0), "l2", "s2", &[]),
            ]),
        );

    let resolver1 = Resolver::new(registry.clone());
    let resolver2 = Resolver::new(registry.clone());

    let resolved1 = resolver1
        .resolve(
            requires(&[("acme/app", "^1.0")]),
            RequireMap::new(),
            ResolveOptions::default(),
        )
        .await
        .expect("first resolve");

    let resolved2 = resolver2
        .resolve(
            requires(&[("acme/app", "^1.0")]),
            RequireMap::new(),
            ResolveOptions::default(),
        )
        .await
        .expect("second resolve");

    assert_eq!(
        resolved1.len(),
        resolved2.len(),
        "same number of packages resolved"
    );
    for (a, b) in resolved1.iter().zip(resolved2.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.version, b.version);
    }
}
