use lazy_static::lazy_static;
use std::env;

lazy_static! {
    static ref _ENABLE_DEBUG: bool = env::var("ENABLE_DEBUG").is_ok();
}

pub fn enable_debug() -> bool {
    *_ENABLE_DEBUG
}
