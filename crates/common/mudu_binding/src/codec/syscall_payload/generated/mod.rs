//! mgen-generated MSSP syscall codec. Do not edit manually.
//!
//! Regenerate with (from `crates/tools`):
//!
//! ```text
//! cargo run -p mudu_gen -- message -i ../common/mudu_binding/wit \
//!     -o <scratch dir> -l rust --with-func-codec
//! ```
//!
//! then copy `<scratch dir>/uni_syscall.rs` over `generated/uni_syscall.rs`
//! (and the `uni_*.rs` universal types over `src/universal/`). See
//! `REGENERATE.md` in the parent directory for the full workflow.

pub mod uni_syscall;
