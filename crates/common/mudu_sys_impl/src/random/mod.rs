//! Random value and UUID generation.
pub use crate::imp::random::Uuid;

/// Generates a random version-4 UUID using the default system environment.
pub fn uuid_v4() -> Uuid {
    crate::default_env().random().uuid_v4()
}

/// Generates a random version-4 UUID and returns its hyphenated string form.
pub fn next_uuid_v4_string() -> String {
    uuid_v4().to_string()
}
