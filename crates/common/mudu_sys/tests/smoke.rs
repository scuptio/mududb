//! Smoke tests for the `mudu_sys` facade.
#![allow(clippy::unwrap_used)]

use mudu_sys::random::uuid_v4;
use mudu_sys::task::sync::sleep_blocking;
use mudu_sys::time::instant_now;
use std::time::Duration;

/// Smoke test that the native system facade exposes working time/random helpers.
#[test]
#[cfg(not(target_arch = "wasm32"))]
fn time_and_random_helpers_work() {
    let before = instant_now();
    sleep_blocking(Duration::from_millis(5));
    let after = instant_now();
    assert!(after > before, "instant should advance");

    let uuid1 = uuid_v4();
    let uuid2 = uuid_v4();
    assert_ne!(uuid1, uuid2, "random uuids should differ");
    assert_eq!(uuid1.get_version_num(), 4, "expected version 4 uuid");
}
