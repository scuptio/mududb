use crate::imp::env::Sys;

pub fn exit(code: i32) -> ! {
    Sys::exit(code)
}
