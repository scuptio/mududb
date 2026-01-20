#[cfg(all(target_arch = "wasm32", feature = "macro"))]
pub mod wasm;

#[cfg(target_arch = "x86_64")]
pub mod wasm_mtp;
#[cfg(all(target_arch = "wasm32", feature = "transpile"))]
pub mod generated;
