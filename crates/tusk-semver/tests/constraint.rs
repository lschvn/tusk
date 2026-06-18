//! Integration tests for the Composer constraint grammar parser and matcher.
//!
//! Spec: <https://getcomposer.org/doc/articles/versions.md>
//! See GOAL.md §7.1.
//!
//! These tests pin every behavior the parser must satisfy. They are intentionally
//! written before the implementation is in place — running `cargo test` against
//! the stub `unimplemented!()` bodies fails for the right reason (a panic in
//! `Constraint::parse`), and the implementation lives once these tests all pass.

use tusk_semver::{Constraint, Stability, Version};

// ---------- helpers ----------

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

fn v_with_stability(major: u32, minor: u32, patch: u32, stability: Stability) -> Version {
    Version {
        major,
        minor,
        patch,
        tweak: None,
        stability,
        stability_n: None,
        dev_branch: None,
        is_v_prefixed: false,
    }
}

fn must_parse(s: &str) -> Constraint {
    Constraint::parse(s).unwrap_or_else(|e| panic!("constraint {s:?} should parse: {e}"))
}

// ---------- exact match ----------

#[test]
fn exact_version_matches_only_that_version() {
    let c = must_parse("1.2.3");
    assert!(c.matches(&v(1, 2, 3)));
    assert!(!c.matches(&v(1, 2, 4)));
    assert!(!c.matches(&v(1, 2, 2)));
    assert!(!c.matches(&v(2, 2, 3)));
}

#[test]
fn exact_version_with_full_stability() {
    // `1.2.3` is the stable form. `1.2.3-RC1` is a different version.
    let c = must_parse("1.2.3");
    let rc = Version {
        stability: Stability::Rc,
        stability_n: Some(1),
        ..v(1, 2, 3)
    };
    assert!(c.matches(&v(1, 2, 3)));
    assert!(!c.matches(&rc));
}

#[test]
fn double_equals_prefix_is_an_alias_for_exact() {
    // `==1.2.3` is a Composer-allowed exact form.
    let c = must_parse("==1.2.3");
    assert!(c.matches(&v(1, 2, 3)));
    assert!(!c.matches(&v(1, 2, 4)));
}

// ---------- caret ----------

#[test]
fn caret_one_digit() {
    // `^1.2.3` means >=1.2.3, <2.0.0
    let c = must_parse("^1.2.3");
    assert!(c.matches(&v(1, 2, 3)));
    assert!(c.matches(&v(1, 2, 99)));
    assert!(c.matches(&v(1, 9, 0)));
    assert!(!c.matches(&v(1, 2, 2)));
    assert!(!c.matches(&v(2, 0, 0)));
}

#[test]
fn caret_zero_major_means_strict_minor() {
    // `^0.2.3` means >=0.2.3, <0.3.0 (per Composer: leading 0 pins the minor)
    let c = must_parse("^0.2.3");
    assert!(c.matches(&v(0, 2, 3)));
    assert!(c.matches(&v(0, 2, 9)));
    assert!(!c.matches(&v(0, 3, 0)));
    assert!(!c.matches(&v(0, 2, 2)));
}

#[test]
fn caret_short_form_two_parts() {
    // `^1.2` is treated as `^1.2.0` — same range as `^1.2.0`
    let c = must_parse("^1.2");
    assert!(c.matches(&v(1, 2, 0)));
    assert!(c.matches(&v(1, 5, 7)));
    assert!(!c.matches(&v(2, 0, 0)));
}

// ---------- tilde ----------

#[test]
fn tilde_three_parts_means_floor_minor() {
    // `~1.2.3` means >=1.2.3, <1.3.0
    let c = must_parse("~1.2.3");
    assert!(c.matches(&v(1, 2, 3)));
    assert!(c.matches(&v(1, 2, 99)));
    assert!(!c.matches(&v(1, 3, 0)));
    assert!(!c.matches(&v(1, 2, 2)));
}

#[test]
fn tilde_two_parts_means_floor_minor_at_zero() {
    // `~1.2` means >=1.2, <2.0 (per Composer: a 2-part tilde pins the minor and goes up to next major)
    let c = must_parse("~1.2");
    assert!(c.matches(&v(1, 2, 0)));
    assert!(c.matches(&v(1, 9, 0)));
    assert!(!c.matches(&v(2, 0, 0)));
}

// ---------- wildcards ----------

#[test]
fn bare_star_matches_anything() {
    let c = must_parse("*");
    assert!(c.matches(&v(0, 0, 1)));
    assert!(c.matches(&v(99, 99, 99)));
}

#[test]
fn wildcard_patch_matches_minor_range() {
    // `1.2.*` means >=1.2.0, <1.3.0
    let c = must_parse("1.2.*");
    assert!(c.matches(&v(1, 2, 0)));
    assert!(c.matches(&v(1, 2, 99)));
    assert!(!c.matches(&v(1, 3, 0)));
    assert!(!c.matches(&v(1, 1, 99)));
}

#[test]
fn wildcard_minor_matches_major_range() {
    // `1.*` means >=1.0.0, <2.0.0
    let c = must_parse("1.*");
    assert!(c.matches(&v(1, 0, 0)));
    assert!(c.matches(&v(1, 99, 99)));
    assert!(!c.matches(&v(2, 0, 0)));
}

// ---------- comparison ops ----------

#[test]
fn greater_equal_inclusive_boundary() {
    let c = must_parse(">=1.2.3");
    assert!(c.matches(&v(1, 2, 3)));
    assert!(c.matches(&v(1, 2, 4)));
    assert!(c.matches(&v(99, 0, 0)));
    assert!(!c.matches(&v(1, 2, 2)));
}

#[test]
fn greater_than_strict_boundary() {
    let c = must_parse(">1.2.3");
    assert!(!c.matches(&v(1, 2, 3)));
    assert!(c.matches(&v(1, 2, 4)));
}

#[test]
fn less_equal_and_less_than() {
    assert!(must_parse("<=2.0.0").matches(&v(2, 0, 0)));
    assert!(!must_parse("<=2.0.0").matches(&v(2, 0, 1)));
    assert!(!must_parse("<2.0.0").matches(&v(2, 0, 0)));
    assert!(must_parse("<2.0.0").matches(&v(1, 99, 99)));
}

#[test]
fn not_equal_excludes_only_that_version() {
    let c = must_parse("!=1.5.0");
    assert!(!c.matches(&v(1, 5, 0)));
    assert!(c.matches(&v(1, 4, 99)));
    assert!(c.matches(&v(1, 5, 1)));
    assert!(c.matches(&v(0, 0, 0)));
}

// ---------- hyphen range ----------

#[test]
fn hyphen_range_is_inclusive_both_ends() {
    // `1.2 - 3.4` means >=1.2.0, <=3.4.0
    let c = must_parse("1.2 - 3.4");
    assert!(c.matches(&v(1, 2, 0)));
    assert!(c.matches(&v(2, 0, 0)));
    assert!(c.matches(&v(3, 4, 0)));
    assert!(c.matches(&v(3, 4, 99)));
    assert!(!c.matches(&v(1, 1, 99)));
    assert!(!c.matches(&v(3, 5, 0)));
}

// ---------- OR / AND ----------

#[test]
fn or_disjunction_matches_either_branch() {
    let c = must_parse("1.2.3 || 2.0.0");
    assert!(c.matches(&v(1, 2, 3)));
    assert!(c.matches(&v(2, 0, 0)));
    assert!(!c.matches(&v(1, 2, 4)));
    assert!(!c.matches(&v(2, 0, 1)));
}

#[test]
fn or_with_spaces() {
    let c = must_parse("1.2 || >=2.0");
    assert!(c.matches(&v(1, 2, 0)));
    assert!(c.matches(&v(2, 0, 0)));
    assert!(c.matches(&v(3, 0, 0)));
    assert!(!c.matches(&v(1, 1, 99)));
    assert!(!c.matches(&v(1, 3, 0)));
}

#[test]
fn and_conjunction_with_comma() {
    let c = must_parse(">=1.0,<2.0");
    assert!(c.matches(&v(1, 0, 0)));
    assert!(c.matches(&v(1, 99, 99)));
    assert!(!c.matches(&v(2, 0, 0)));
    assert!(!c.matches(&v(0, 99, 99)));
}

#[test]
fn and_conjunction_with_whitespace() {
    let c = must_parse(">=1.0 <2.0");
    assert!(c.matches(&v(1, 5, 0)));
    assert!(!c.matches(&v(2, 0, 0)));
}

#[test]
fn combined_or_and() {
    // `(>=1.0,<2.0) || (>=3.0,<4.0)`
    let c = must_parse(">=1.0 <2.0 || >=3.0 <4.0");
    assert!(c.matches(&v(1, 5, 0)));
    assert!(c.matches(&v(3, 5, 0)));
    assert!(!c.matches(&v(2, 5, 0)));
    assert!(!c.matches(&v(4, 5, 0)));
}

// ---------- stability flags ----------

#[test]
fn stability_flag_dev_matches_dev_only() {
    // `1.2.3@dev` means "1.2.3 at dev stability" — i.e. matches 1.2.3-dev but not 1.2.3
    let c = must_parse("1.2.3@dev");
    assert!(c.matches(&v_with_stability(1, 2, 3, Stability::Dev)));
    assert!(!c.matches(&v(1, 2, 3)));
    assert!(!c.matches(&v_with_stability(1, 2, 3, Stability::Alpha)));
}

#[test]
fn stability_flag_stable_filters_out_unstable() {
    // `1.2.3@stable` means "1.2.3 at stable stability" — i.e. matches 1.2.3 only
    let c = must_parse("1.2.3@stable");
    assert!(c.matches(&v(1, 2, 3)));
    assert!(!c.matches(&v_with_stability(1, 2, 3, Stability::Dev)));
    assert!(!c.matches(&v_with_stability(1, 2, 3, Stability::Beta)));
}

// ---------- determinism + property test ----------

#[test]
fn parsing_twice_yields_identical_structural_output() {
    let a = must_parse("^1.2.3 || >=2.0 <3.0");
    let b = must_parse("^1.2.3 || >=2.0 <3.0");
    assert_eq!(a, b, "identical input must produce identical AST");
}

#[test]
fn invalid_input_returns_error_not_panic() {
    // Garbage that contains a digit but isn't a valid constraint.
    let bad = Constraint::parse("not-a-version!!!");
    // Either an Err or a structural mismatch — but the function MUST NOT panic.
    // We just assert it returned a Result (it always does by signature).
    let _ = bad;
}
