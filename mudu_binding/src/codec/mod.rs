//! Serialization codecs that convert between `mudu` core types and their
//! universal byte/json representations.

mod adapter;

pub(crate) mod handle_procedure;
pub(crate) mod handle_sys_command;
pub mod handle_sys_fs;
/// Decoding of incoming command/query arguments.
pub mod handle_sys_incoming;
/// Encoding of outgoing query results.
pub mod handle_sys_outcoming;
pub(crate) mod handle_sys_query;
pub mod handle_sys_session;
/// SyscallPayload v1 codec: 16-byte MSSP header + MessagePack bodies for the
/// `uni-syscall.wit` syscall surface (see `doc/cn/contract/syscall_payload_v1.md`).
pub mod syscall_payload;
use mudu::common::id::OID;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

/// A decoded SQL parameter pair: database OID, statement and bound parameters.
pub type SqlParamPair = (OID, Box<dyn SQLStmt>, Box<dyn SQLParams>);
