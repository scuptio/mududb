use crate::imp::env::Sys;
use lazy_static::lazy_static;

lazy_static! {
    static ref DefaultSys: Sys = Sys::new();
}

/// Returns the process-wide default native system environment.
///
/// The environment is constructed lazily on first use and shared by all
/// callers for the lifetime of the process.
pub fn default_env() -> &'static Sys {
    &DefaultSys
}
