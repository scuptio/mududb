#[allow(unused)]
#[allow(dead_code)]
#[cfg(target_arch = "x86_64")]
pub mod rust;

#[allow(unused)]
#[cfg(target_arch = "wasm32")]
pub mod generated;

#[cfg(all(test, target_arch = "x86_64"))]
mod testing;
