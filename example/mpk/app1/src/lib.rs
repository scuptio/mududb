//! Test procedure package (`app1`) for the mudu_runtime component/procedure
//! tests.
//!
//! This crate rebuilds the prebuilt `mudu_runtime/data/wasm_module/app1.mpk`
//! guest package from source: three trivial procedures (`proc_mtp`,
//! `proc2_mtp`, `proc_sys_call_mtp`) matching the historical package
//! descriptor in `package/package.desc.json`.

#![warn(missing_docs)]
#![allow(dead_code)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]
#![cfg_attr(
    test,
    allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

#[allow(unused)]
#[cfg(target_arch = "x86_64")]
pub mod rust;

#[allow(unused, missing_docs)]
#[cfg(target_arch = "wasm32")]
pub mod generated;

/// Test-only subset of the generated module. The full generated procedure file
/// contains pre-existing async `#[test]` functions that do not compile on
/// native targets, so unit tests expose only the generated entity modules.
#[allow(unused, missing_docs)]
#[cfg(all(test, target_arch = "x86_64"))]
pub mod generated {
    pub mod wallets;
}

#[cfg(test)]
mod generated_test;
