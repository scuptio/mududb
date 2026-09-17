//! `mudu_gen`-generated MSSP syscall codec.
//!
//! `uni_syscall` is produced by `mudu_gen` (`mgen`) from the WIT contracts in
//! `crates/common/mudu_binding/wit` and must not be edited manually; see
//! `REGENERATE.md` in the crate root for the regeneration command. The
//! generated code refers to the universal DTOs through `crate::universal::...`
//! paths and stays self-contained: this crate cannot depend on
//! `mudu_binding`, so the generated file is the shared source of truth for the
//! wire format.

// The generated codec contains both halves of every request/result pair; a
// pure guest build (without `mock-sqlite`) never calls the host-side
// `decode_*_request` / `encode_*_result` half, so dead-code analysis would
// flag generated functions that must stay for the mock and for symmetry with
// the host.
#![allow(dead_code)]

pub mod uni_syscall;
