//! Integration tests for `composer.json` parse + serialize.
//!
//! Fixtures are committed in `fixtures/manifest/` at the workspace root and
//! embedded at compile time via `include_str!` so tests are hermetic.

use tusk_manifest::ComposerJson;

const MINIMAL: &str = include_str!("../../../fixtures/manifest/minimal.json");
const MULTIPATH_PSR4: &str = include_str!("../../../fixtures/manifest/multipath-psr4.json");
const AUTOLOAD_FILES: &str = include_str!("../../../fixtures/manifest/autoload-files.json");
const CLASSMAP_PSR0: &str = include_str!("../../../fixtures/manifest/classmap-psr0.json");

#[test]
fn from_str_minimal_succeeds_and_exposes_php_require() {
    let parsed = ComposerJson::from_str(MINIMAL).expect("minimal.json should parse");

    assert_eq!(parsed.name.as_deref(), Some("foo/bar"));
    assert_eq!(
        parsed.require.get("php").map(String::as_str),
        Some("^8.1"),
        "the php constraint from the fixture should be preserved"
    );
    // Defensive: a `require-dev` key in the JSON that is absent must not appear
    // in the parsed map.
    assert!(!parsed.require.contains_key("ext-json"));
}

#[test]
fn from_str_parses_require_dev_separately_from_require() {
    let input = r#"{
        "name": "acme/widget",
        "require": {
            "php": "^8.1"
        },
        "require-dev": {
            "phpunit/phpunit": "^10.0"
        }
    }"#;

    let parsed = ComposerJson::from_str(input).expect("manifest with require-dev should parse");

    assert_eq!(parsed.require.get("php").map(String::as_str), Some("^8.1"));
    assert_eq!(
        parsed
            .require_dev
            .get("phpunit/phpunit")
            .map(String::as_str),
        Some("^10.0"),
        "require-dev must populate its own map and not leak into require"
    );
    assert!(
        !parsed.require.contains_key("phpunit/phpunit"),
        "require-dev entries must not be in require"
    );
}

#[test]
fn from_str_parses_multipath_psr4_autoload() {
    let parsed = ComposerJson::from_str(MULTIPATH_PSR4).expect("multipath-psr4.json should parse");

    let psr4 = parsed
        .autoload
        .get("psr-4")
        .expect("autoload should have a psr-4 section")
        .as_object()
        .expect("autoload.psr-4 should be a JSON object");

    // Single-path entry: value is a bare string.
    let core = psr4
        .get("Acme\\Core\\")
        .expect("Acme\\Core\\ must be present")
        .as_str()
        .expect("Acme\\Core\\ value must be a string");
    assert_eq!(core, "src/Core/");

    // Multi-path entry: value is an array of strings. Composer accepts both a
    // string and an array of strings as the value of a PSR-4 mapping.
    let tests = psr4
        .get("Acme\\Tests\\")
        .expect("Acme\\Tests\\ must be present")
        .as_array()
        .expect("Acme\\Tests\\ value must be a JSON array of paths");
    let tests_paths: Vec<&str> = tests.iter().filter_map(serde_json::Value::as_str).collect();
    assert_eq!(tests_paths, vec!["tests/Core/", "tests/Integration/"]);
}

#[test]
fn from_str_parses_autoload_files_array() {
    let parsed = ComposerJson::from_str(AUTOLOAD_FILES).expect("autoload-files.json should parse");

    let files = parsed
        .autoload
        .get("files")
        .expect("autoload should have a 'files' section")
        .as_array()
        .expect("autoload.files should be a JSON array");

    let file_paths: Vec<&str> = files.iter().filter_map(serde_json::Value::as_str).collect();
    assert_eq!(
        file_paths,
        vec!["src/helpers.php", "src/polyfills.php"],
        "the order of files must be preserved"
    );
}

#[test]
fn from_str_parses_classmap_and_psr0_autoload() {
    let parsed = ComposerJson::from_str(CLASSMAP_PSR0).expect("classmap-psr0.json should parse");

    // psr-0: a JSON object mapping prefix -> string path.
    let psr0 = parsed
        .autoload
        .get("psr-0")
        .expect("autoload.psr-0 should be present")
        .as_object()
        .expect("autoload.psr-0 must be a JSON object");
    assert_eq!(
        psr0.get("Zend_").and_then(serde_json::Value::as_str),
        Some("library/")
    );

    // classmap: a JSON array of path strings (directories or single files).
    let classmap = parsed
        .autoload
        .get("classmap")
        .expect("autoload.classmap should be present")
        .as_array()
        .expect("autoload.classmap must be a JSON array");
    let classmap_paths: Vec<&str> = classmap
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    assert_eq!(classmap_paths, vec!["src/Legacy", "ext/legacy/Compat.php"]);

    // exclude-from-classmap: also a JSON array of strings.
    let excluded = parsed
        .autoload
        .get("exclude-from-classmap")
        .expect("autoload.exclude-from-classmap should be present")
        .as_array()
        .expect("autoload.exclude-from-classmap must be a JSON array");
    let excluded_paths: Vec<&str> = excluded
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    assert_eq!(excluded_paths, vec!["src/Legacy/Internal"]);
}
