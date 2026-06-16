use crate::imp::env::Sys;
use uuid::Uuid;

pub fn uuid_v4() -> Uuid {
    Sys::uuid_v4()
}

pub fn next_uuid_v4_string() -> String {
    uuid_v4().to_string()
}
