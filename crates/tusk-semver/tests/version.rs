//! Integration tests for `Version::parse` and `Version::to_composer_string`.

use tusk_semver::{Stability, Version};

#[test]
fn parse_basic_three_part() {
    let v = Version::parse("1.2.3").expect("1.2.3 must parse");
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 2);
    assert_eq!(v.patch, 3);
    assert_eq!(v.tweak, None);
    assert_eq!(v.stability, Stability::Stable);
    assert_eq!(v.stability_n, None);
    assert_eq!(v.dev_branch, None);
    assert!(!v.is_v_prefixed);
}

#[test]
fn parse_v_prefix_tolerated_and_stripped_on_output() {
    let v = Version::parse("v2.5.0").expect("v2.5.0 must parse");
    assert_eq!(v.major, 2);
    assert_eq!(v.minor, 5);
    assert_eq!(v.patch, 0);
    assert!(v.is_v_prefixed, "v-prefix on input should be recorded");
    assert_eq!(
        v.to_composer_string(),
        "2.5.0",
        "v-prefix must not appear on output"
    );
}

#[test]
fn parse_four_component_sets_tweak() {
    let v = Version::parse("1.2.3.4").expect("1.2.3.4 must parse");
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 2);
    assert_eq!(v.patch, 3);
    assert_eq!(v.tweak, Some(4));
    assert_eq!(v.stability, Stability::Stable);
}

#[test]
fn parse_stability_suffixes() {
    let cases: &[(&str, Stability, Option<u32>)] = &[
        ("1.2.3-alpha", Stability::Alpha, None),
        ("1.2.3-alpha.2", Stability::Alpha, Some(2)),
        ("1.2.3-beta.1", Stability::Beta, Some(1)),
        ("1.2.3-RC1", Stability::Rc, Some(1)),
        ("1.2.3-dev", Stability::Dev, None),
        ("1.2.3-pl3", Stability::Stable, Some(3)),
    ];
    for (input, expected_stab, expected_n) in cases {
        let v = Version::parse(input).unwrap_or_else(|e| panic!("{input} should parse: {e:?}"));
        assert_eq!(v.stability, *expected_stab, "stability for {input}");
        assert_eq!(v.stability_n, *expected_n, "stability_n for {input}");
    }
}
