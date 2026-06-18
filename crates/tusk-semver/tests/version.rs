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
