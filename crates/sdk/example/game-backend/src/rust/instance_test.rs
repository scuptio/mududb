//! Unit tests for the native game-backend instance cache.

use crate::rust::game_object::GameObject;
use crate::rust::instance::Instance;

#[test]
fn instance_put_and_get_roundtrip() {
    let object = GameObject { id: 42 };
    Instance::put(1, object.clone());
    assert_eq!(Instance::get(1).map(|o| o.id), Some(42));
}

#[test]
fn instance_get_missing_returns_none() {
    // The cache is thread-local; a fresh test thread starts empty.
    assert!(Instance::get(9999).is_none());
}
