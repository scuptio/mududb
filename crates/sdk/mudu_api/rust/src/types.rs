//! Syscall result types exchanged with the host.
//!
//! The wire-level result variants are the generated universal mirrors:
//! `UniCommandReturn` / `UniQueryReturn` encode as the MessagePack two-array
//! `[tag, payload]` (`0` = ok, `1` = err) defined by
//! `doc/cn/contract/syscall_payload_v1.md`.

pub use crate::universal::uni_command_return::{UniCommandResult, UniCommandReturn};
pub use crate::universal::uni_query_return::UniQueryReturn;

use crate::universal::uni_error::UniError;

/// Wire-level `result<T, uni-error>` shared by the syscalls that have no
/// dedicated `Uni*Return` enum: `Ok(value)` encodes as `[0u8, value]` and
/// `Err(UniError)` as `[1u8, UniError]`.
pub type UniReturn<T> = Result<T, UniError>;
