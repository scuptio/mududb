//! Syscall result types exchanged with the host.
//!
//! The wire-level result variants are the generated universal mirrors:
//! `UniCommandReturn` / `UniQueryReturn` encode as the MessagePack two-array
//! `[tag, payload]` (`0` = ok, `1` = err) defined by
//! `doc/cn/contract/syscall_payload_v1.md`.

pub use crate::universal::uni_command_return::{UniCommandResult, UniCommandReturn};
pub use crate::universal::uni_query_return::UniQueryReturn;
