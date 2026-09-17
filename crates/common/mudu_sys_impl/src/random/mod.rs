//! Random value and UUID generation.
#![allow(missing_docs)]
pub use crate::imp::random::Uuid;

pub fn uuid_v4() -> Uuid {
    crate::default_env().random().uuid_v4()
}

pub fn next_uuid_v4_string() -> String {
    uuid_v4().to_string()
}
