//! Integration tests for `composer.json` parse + serialize.
//!
//! Fixtures are committed in `fixtures/manifest/` at the workspace root and
//! embedded at compile time via `include_str!` so tests are hermetic.

use tusk_manifest::ComposerJson;

const MINIMAL: &str = include_str!("../../../fixtures/manifest/minimal.json");

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
