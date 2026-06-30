//! Tests for the `sync` module.
#![allow(clippy::unwrap_used)]

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

/// `SMutex::new` stores the provided value and `lock` exposes it mutably.
#[test]
fn smutex_new_and_lock() {
    let mutex = super::sync::SMutex::new(42);
    {
        let mut guard = mutex.lock().unwrap();
        assert_eq!(*guard, 42);
        *guard += 1;
    }
    assert_eq!(*mutex.lock().unwrap(), 43);
}

/// `try_lock` succeeds when the mutex is not held and fails when it is.
#[test]
fn smutex_try_lock_succeeds_and_fails_as_expected() {
    let mutex = super::sync::SMutex::new("value");
    let guard = mutex.try_lock().expect("should acquire unlocked mutex");
    assert_eq!(*guard, "value");

    // While locked, another `try_lock` must return `None`.
    assert!(mutex.try_lock().is_none());
}

/// `SMutex` derives `Default` for types that implement `Default`.
#[test]
fn smutex_default_is_zeroed() {
    let mutex = <super::sync::SMutex<i32> as Default>::default();
    assert_eq!(*mutex.lock().unwrap(), 0);
}

/// A poisoned mutex returns an error from `lock` and `None` from `try_lock`.
#[test]
fn smutex_lock_reports_poison_after_panic() {
    let mutex = Arc::new(super::sync::SMutex::new(0));
    let mutex2 = Arc::clone(&mutex);

    let result = catch_unwind(AssertUnwindSafe(move || {
        let mut guard = mutex2.lock().unwrap();
        *guard = 1;
        panic!("intentional poison");
    }));
    assert!(result.is_err());

    assert!(mutex.lock().is_err());
    assert!(mutex.try_lock().is_none());
}
