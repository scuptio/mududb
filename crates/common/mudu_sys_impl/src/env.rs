#![allow(missing_docs)]
use crate::imp::env::Sys;
use lazy_static::lazy_static;

lazy_static! {
    static ref DefaultSys: Sys = Sys::new();
}

pub fn default_env() -> &'static Sys {
    &DefaultSys
}
