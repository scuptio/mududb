// mudu_sys is a target-selecting facade crate.
// On native targets the `native` feature (default) re-exports mudu_sys_impl.
// The empty `ds` feature is an internal-only hook: a closed-source
// deterministic simulation overlay adds the backend dependency and the
// corresponding re-export; in this repository enabling `ds` is a no-op.
#[cfg(all(not(target_arch = "wasm32"), feature = "native"))]
pub use mudu_sys_impl::*;

#[cfg(target_arch = "wasm32")]
pub use mudu_sys_wasm::*;
