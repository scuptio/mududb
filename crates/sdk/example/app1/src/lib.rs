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

#[allow(unused)]
#[cfg(target_arch = "x86_64")]
pub mod rust;

#[allow(unused, missing_docs)]
#[cfg(target_arch = "wasm32")]
pub mod generated;
