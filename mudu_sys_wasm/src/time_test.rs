//! Tests for the `time` module.
#![allow(clippy::unwrap_used)]

use chrono::Datelike;
use std::time::Duration;

/// `instant_now` increases after sleeping on the current thread.
#[test]
fn instant_now_increases_monotonically() {
    let before = super::time::instant_now();
    std::thread::sleep(Duration::from_millis(5));
    let after = super::time::instant_now();
    assert!(after > before);
}

/// `system_time_now` is after the UNIX epoch.
#[test]
fn system_time_now_is_after_unix_epoch() {
    assert!(super::time::system_time_now() > std::time::SystemTime::UNIX_EPOCH);
}

/// `utc_now` returns a recent UTC timestamp.
#[test]
fn utc_now_returns_recent_time() {
    let now = super::time::utc_now();
    assert!(now.timestamp() > 0);
    assert!(now.year() >= 2024);

    // The reported time should not be in the future.
    assert!(now <= super::time::Utc::now());
}

/// The re-exported time types can be named through the `time` module.
#[test]
fn time_re_exports_are_usable() {
    let _instant: super::time::Instant = super::time::instant_now();
    let _system_time: super::time::SystemTime = super::time::system_time_now();
    let _utc: super::time::DateTime<super::time::Utc> = super::time::utc_now();
}
