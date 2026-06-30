// mudu_sys is a target-selecting facade crate.
#[cfg(not(target_arch = "wasm32"))]
pub use mudu_sys_impl::*;

#[cfg(target_arch = "wasm32")]
pub use mudu_sys_wasm::*;
