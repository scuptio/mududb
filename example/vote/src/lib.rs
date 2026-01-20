#[allow(unused)]
#[cfg(target_arch="x86_64")]
pub mod rust;

#[allow(unused)]
#[cfg(target_arch = "wasm32")]
pub mod generated;