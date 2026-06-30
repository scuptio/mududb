//! Tests for the `random` module.
#![allow(clippy::unwrap_used)]

/// `uuid_v4` produces distinct version-4 UUIDs.
#[test]
fn uuid_v4_returns_distinct_version_4_values() {
    let a = super::random::uuid_v4();
    let b = super::random::uuid_v4();
    assert_ne!(a, b);
    assert_eq!(a.get_version_num(), 4);
    assert_eq!(b.get_version_num(), 4);
}

/// `next_uuid_v4_string` returns a valid, parseable version-4 UUID string.
#[test]
fn next_uuid_v4_string_returns_valid_uuid_string() {
    let s = super::random::next_uuid_v4_string();
    assert_eq!(s.len(), 36);
    assert_eq!(s.chars().filter(|&c| c == '-').count(), 4);

    let parsed = super::random::Uuid::parse_str(&s).unwrap();
    assert_eq!(parsed.get_version_num(), 4);
}
