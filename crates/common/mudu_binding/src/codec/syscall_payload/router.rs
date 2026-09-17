//! Per-kind request/result codecs for the 23 `uni-syscall.wit` functions.
//!
//! Every `encode_*` function returns a complete MSSP frame (16-byte header
//! included); every `decode_*` function consumes a complete frame, validates
//! the header and rejects frames whose message kind does not match.
//!
//! Request bodies are MessagePack arrays of the WIT-declared positional
//! arguments (a single argument is a one-element array; records nest as
//! their own array). Result bodies are `[ok_tag, value]` pairs; unit results
//! use the `[0u8, 0u8]` placeholder form. See
//! [`crate::codec::syscall_payload`] for the frame layout and integrity
//! rules.
//!
//! This module is a thin `RS<...>`/`MuduError` adapter over the
//! `mudu_gen`-generated codec in [`super::generated`]: the public function
//! names and signatures are the pre-generation host-side API, while every
//! byte on the wire is produced/validated by the generated code. Decode
//! wrappers pre-validate the header with
//! [`super::decode_typed_frame`] so header violations keep the exact
//! `mudu::compat` error codes; the generated decoder then only fails on
//! malformed bodies, which map to `ErrorCode::Decode`.

use super::generated::uni_syscall as gen_codec;
use super::{MessageKind, decode_typed_frame, frame_error_to_mudu};
use crate::codec::adapter::{error_from_mu, error_to_mu, oid_from_mu, oid_to_mu};
use crate::universal::uni_command_argv::UniCommandArgv;
use crate::universal::uni_command_return::UniCommandResult;
use crate::universal::uni_fs_dirent::UniFsDirent;
use crate::universal::uni_fs_open_argv::UniFsOpenArgv;
use crate::universal::uni_fs_stat::UniFsStat;
use crate::universal::uni_oid::UniOid;
use crate::universal::uni_query_argv::UniQueryArgv;
use crate::universal::uni_query_result::UniQueryResult;
use mudu::common::id::OID;
use mudu::common::result::RS;

/// Converts a host-side `RS<T>` into the generated codec's
/// `UniResult<&T>` (`Result<&T, UniError>`).
///
/// The success value is borrowed: the generated result encoders are generic
/// over `T: Borrow<OkT>`, so the payload is serialized in place without a
/// clone; the error arm is converted to the universal error record either
/// way.
fn rs_to_uni<T>(result: &RS<T>) -> gen_codec::UniResult<&T> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => Err(error_to_mu(error.clone())),
    }
}

/// Unit-result variant of [`rs_to_uni`]: the generated unit encoders take
/// `&UniResult<()>`, so the success arm maps `Ok(())` through unchanged.
fn rs_to_uni_unit(result: &RS<()>) -> gen_codec::UniResult<()> {
    match result {
        Ok(()) => Ok(()),
        Err(error) => Err(error_to_mu(error.clone())),
    }
}

/// Collapses the generated decoder's two-level result
/// (`Result<UniResult<T>, SyscallFrameError>`) into a single `RS<T>`.
fn from_gen<T>(decoded: Result<gen_codec::UniResult<T>, gen_codec::SyscallFrameError>) -> RS<T> {
    decoded.map_err(frame_error_to_mudu)?.map_err(error_from_mu)
}

// ---- query ----

/// Encodes a `query` request frame: body `[argv]`.
pub fn encode_query_request(argv: &UniQueryArgv) -> Vec<u8> {
    gen_codec::encode_query_request(argv)
}

/// Decodes a `query` request frame into its argument record.
pub fn decode_query_request(frame: &[u8]) -> RS<UniQueryArgv> {
    decode_typed_frame(MessageKind::Query, frame)?;
    gen_codec::decode_query_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes a `query` result frame: body `[0, UniQueryResult]` or
/// `[1, UniError]`.
pub fn encode_query_result(result: &RS<UniQueryResult>) -> Vec<u8> {
    gen_codec::encode_query_result_ref(&rs_to_uni(result))
}

/// Decodes a `query` result frame.
pub fn decode_query_result(frame: &[u8]) -> RS<UniQueryResult> {
    decode_typed_frame(MessageKind::Query, frame)?;
    from_gen(gen_codec::decode_query_result(frame))
}

// ---- command ----

/// Encodes a `command` request frame: body `[argv]`.
pub fn encode_command_request(argv: &UniCommandArgv) -> Vec<u8> {
    gen_codec::encode_command_request(argv)
}

/// Decodes a `command` request frame into its argument record.
pub fn decode_command_request(frame: &[u8]) -> RS<UniCommandArgv> {
    decode_typed_frame(MessageKind::Command, frame)?;
    gen_codec::decode_command_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes a `command` result frame: body `[0, UniCommandResult]` or
/// `[1, UniError]`.
pub fn encode_command_result(result: &RS<UniCommandResult>) -> Vec<u8> {
    gen_codec::encode_command_result_ref(&rs_to_uni(result))
}

/// Decodes a `command` result frame.
pub fn decode_command_result(frame: &[u8]) -> RS<UniCommandResult> {
    decode_typed_frame(MessageKind::Command, frame)?;
    from_gen(gen_codec::decode_command_result(frame))
}

// ---- batch ----

/// Encodes a `batch` request frame: body `[argv]`.
pub fn encode_batch_request(argv: &UniCommandArgv) -> Vec<u8> {
    gen_codec::encode_batch_request(argv)
}

/// Decodes a `batch` request frame into its argument record.
pub fn decode_batch_request(frame: &[u8]) -> RS<UniCommandArgv> {
    decode_typed_frame(MessageKind::Batch, frame)?;
    gen_codec::decode_batch_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes a `batch` result frame: body `[0, UniCommandResult]` or
/// `[1, UniError]`.
pub fn encode_batch_result(result: &RS<UniCommandResult>) -> Vec<u8> {
    gen_codec::encode_batch_result_ref(&rs_to_uni(result))
}

/// Decodes a `batch` result frame.
pub fn decode_batch_result(frame: &[u8]) -> RS<UniCommandResult> {
    decode_typed_frame(MessageKind::Batch, frame)?;
    from_gen(gen_codec::decode_batch_result(frame))
}

// ---- open-session ----

/// Encodes an `open-session` request frame: body `[worker_id]`.
pub fn encode_open_request(worker_id: UniOid) -> Vec<u8> {
    gen_codec::encode_open_session_request(&worker_id)
}

/// Decodes an `open-session` request frame into the worker OID.
pub fn decode_open_request(frame: &[u8]) -> RS<UniOid> {
    decode_typed_frame(MessageKind::Open, frame)?;
    gen_codec::decode_open_session_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes an `open-session` result frame: body `[0, UniOid]` or
/// `[1, UniError]`.
pub fn encode_open_result(result: &RS<OID>) -> Vec<u8> {
    let uni = match result {
        Ok(oid) => Ok(oid_to_mu(*oid)),
        Err(error) => Err(error_to_mu(error.clone())),
    };
    gen_codec::encode_open_session_result(&uni)
}

/// Decodes an `open-session` result frame into the new session OID.
pub fn decode_open_result(frame: &[u8]) -> RS<OID> {
    decode_typed_frame(MessageKind::Open, frame)?;
    let oid = from_gen(gen_codec::decode_open_session_result(frame))?;
    Ok(oid_from_mu(oid))
}

// ---- close-session ----

/// Encodes a `close-session` request frame: body `[oid]`.
pub fn encode_close_request(oid: UniOid) -> Vec<u8> {
    gen_codec::encode_close_session_request(&oid)
}

/// Decodes a `close-session` request frame into the session OID.
pub fn decode_close_request(frame: &[u8]) -> RS<UniOid> {
    decode_typed_frame(MessageKind::Close, frame)?;
    gen_codec::decode_close_session_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes a `close-session` result frame: body `[0, 0]` or `[1, UniError]`.
pub fn encode_close_result(result: &RS<()>) -> Vec<u8> {
    gen_codec::encode_close_session_result(&rs_to_uni_unit(result))
}

/// Decodes a `close-session` result frame.
pub fn decode_close_result(frame: &[u8]) -> RS<()> {
    decode_typed_frame(MessageKind::Close, frame)?;
    from_gen(gen_codec::decode_close_session_result(frame))
}

// ---- get ----

/// Encodes a `get` request frame: body `[oid, key]`.
pub fn encode_get_request(oid: UniOid, key: &[u8]) -> Vec<u8> {
    gen_codec::encode_get_request(&oid, key)
}

/// Decodes a `get` request frame into `(oid, key)`.
pub fn decode_get_request(frame: &[u8]) -> RS<(UniOid, Vec<u8>)> {
    decode_typed_frame(MessageKind::Get, frame)?;
    gen_codec::decode_get_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes a `get` result frame: body `[0, value-or-nil]` or `[1, UniError]`.
pub fn encode_get_result(result: &RS<Option<Vec<u8>>>) -> Vec<u8> {
    gen_codec::encode_get_result_ref(&rs_to_uni(result))
}

/// Decodes a `get` result frame into the optional value.
pub fn decode_get_result(frame: &[u8]) -> RS<Option<Vec<u8>>> {
    decode_typed_frame(MessageKind::Get, frame)?;
    from_gen(gen_codec::decode_get_result(frame))
}

// ---- put ----

/// Encodes a `put` request frame: body `[oid, key, value]`.
pub fn encode_put_request(oid: UniOid, key: &[u8], value: &[u8]) -> Vec<u8> {
    gen_codec::encode_put_request(&oid, key, value)
}

/// Decodes a `put` request frame into `(oid, key, value)`.
pub fn decode_put_request(frame: &[u8]) -> RS<(UniOid, Vec<u8>, Vec<u8>)> {
    decode_typed_frame(MessageKind::Put, frame)?;
    gen_codec::decode_put_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes a `put` result frame: body `[0, 0]` or `[1, UniError]`.
pub fn encode_put_result(result: &RS<()>) -> Vec<u8> {
    gen_codec::encode_put_result(&rs_to_uni_unit(result))
}

/// Decodes a `put` result frame.
pub fn decode_put_result(frame: &[u8]) -> RS<()> {
    decode_typed_frame(MessageKind::Put, frame)?;
    from_gen(gen_codec::decode_put_result(frame))
}

// ---- delete ----

/// Encodes a `delete` request frame: body `[oid, key]`.
pub fn encode_delete_request(oid: UniOid, key: &[u8]) -> Vec<u8> {
    gen_codec::encode_delete_request(&oid, key)
}

/// Decodes a `delete` request frame into `(oid, key)`.
pub fn decode_delete_request(frame: &[u8]) -> RS<(UniOid, Vec<u8>)> {
    decode_typed_frame(MessageKind::Delete, frame)?;
    gen_codec::decode_delete_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes a `delete` result frame: body `[0, 0]` or `[1, UniError]`.
pub fn encode_delete_result(result: &RS<()>) -> Vec<u8> {
    gen_codec::encode_delete_result(&rs_to_uni_unit(result))
}

/// Decodes a `delete` result frame.
pub fn decode_delete_result(frame: &[u8]) -> RS<()> {
    decode_typed_frame(MessageKind::Delete, frame)?;
    from_gen(gen_codec::decode_delete_result(frame))
}

// ---- range ----

/// Encodes a `range` request frame: body `[oid, start, end]`.
pub fn encode_range_request(oid: UniOid, start: &[u8], end: &[u8]) -> Vec<u8> {
    gen_codec::encode_range_request(&oid, start, end)
}

/// Decodes a `range` request frame into `(oid, start, end)`.
pub fn decode_range_request(frame: &[u8]) -> RS<(UniOid, Vec<u8>, Vec<u8>)> {
    decode_typed_frame(MessageKind::Range, frame)?;
    gen_codec::decode_range_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes a `range` result frame: body `[0, [[key, value], ...]]` or
/// `[1, UniError]`.
pub fn encode_range_result(result: &RS<Vec<(Vec<u8>, Vec<u8>)>>) -> Vec<u8> {
    gen_codec::encode_range_result_ref(&rs_to_uni(result))
}

/// Decodes a `range` result frame into the key/value pairs.
pub fn decode_range_result(frame: &[u8]) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    decode_typed_frame(MessageKind::Range, frame)?;
    from_gen(gen_codec::decode_range_result(frame))
}

// ---- relation-get ----

/// Encodes a `relation-get` request frame: body
/// `[oid, table, [[attr, datum], ...], [attr, ...]]`.
pub fn encode_relation_get_request(
    oid: UniOid,
    table: &str,
    key: &[(u64, &[u8])],
    select: &[u64],
) -> Vec<u8> {
    let key_owned = key
        .iter()
        .map(|(attr, datum)| (*attr, datum.to_vec()))
        .collect::<Vec<_>>();
    let select_owned = select.to_vec();
    gen_codec::encode_relation_get_request(&oid, table, &key_owned, &select_owned)
}

/// Decoded `relation-get` request payload.
pub type RelationGetRequest = (UniOid, String, Vec<(u64, Vec<u8>)>, Vec<u64>);

/// Decodes a `relation-get` request frame into
/// `(oid, table, key, select)`.
pub fn decode_relation_get_request(frame: &[u8]) -> RS<RelationGetRequest> {
    decode_typed_frame(MessageKind::RelationGet, frame)?;
    gen_codec::decode_relation_get_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes a `relation-get` result frame: body
/// `[0, [[datum-or-nil], ...]-or-nil]` or `[1, UniError]`.
pub fn encode_relation_get_result(result: &RS<Option<Vec<Option<Vec<u8>>>>>) -> Vec<u8> {
    gen_codec::encode_relation_get_result_ref(&rs_to_uni(result))
}

/// Decodes a `relation-get` result frame into the optional projected row.
pub fn decode_relation_get_result(frame: &[u8]) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    decode_typed_frame(MessageKind::RelationGet, frame)?;
    from_gen(gen_codec::decode_relation_get_result(frame))
}

// ---- relation-update ----

/// Encodes a `relation-update` request frame: body
/// `[oid, table, [[attr, datum], ...], [[attr, datum], ...],
/// [[attr, op, datum], ...]]`.
pub fn encode_relation_update_request(
    oid: UniOid,
    table: &str,
    key: &[(u64, &[u8])],
    values: &[(u64, &[u8])],
    deltas: &[(u64, u8, &[u8])],
) -> Vec<u8> {
    let key_owned = key
        .iter()
        .map(|(attr, datum)| (*attr, datum.to_vec()))
        .collect::<Vec<_>>();
    let values_owned = values
        .iter()
        .map(|(attr, datum)| (*attr, datum.to_vec()))
        .collect::<Vec<_>>();
    let deltas_owned = deltas
        .iter()
        .map(|(attr, op, datum)| (*attr, *op, datum.to_vec()))
        .collect::<Vec<_>>();
    gen_codec::encode_relation_update_request(&oid, table, &key_owned, &values_owned, &deltas_owned)
}

/// Decoded `relation-update` request payload.
pub type RelationUpdateRequest = (
    UniOid,
    String,
    Vec<(u64, Vec<u8>)>,
    Vec<(u64, Vec<u8>)>,
    Vec<(u64, u8, Vec<u8>)>,
);

/// Decodes a `relation-update` request frame.
pub fn decode_relation_update_request(frame: &[u8]) -> RS<RelationUpdateRequest> {
    decode_typed_frame(MessageKind::RelationUpdate, frame)?;
    gen_codec::decode_relation_update_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes a `relation-update` result frame: body `[0, affected]` or
/// `[1, UniError]`.
pub fn encode_relation_update_result(result: &RS<u64>) -> Vec<u8> {
    gen_codec::encode_relation_update_result_ref(&rs_to_uni(result))
}

/// Decodes a `relation-update` result frame into the affected row count.
pub fn decode_relation_update_result(frame: &[u8]) -> RS<u64> {
    decode_typed_frame(MessageKind::RelationUpdate, frame)?;
    from_gen(gen_codec::decode_relation_update_result(frame))
}

// ---- relation-insert ----

/// Encodes a `relation-insert` request frame: body
/// `[oid, table, [[attr, datum], ...], [[attr, datum], ...]]`.
pub fn encode_relation_insert_request(
    oid: UniOid,
    table: &str,
    key: &[(u64, &[u8])],
    values: &[(u64, &[u8])],
) -> Vec<u8> {
    let key_owned = key
        .iter()
        .map(|(attr, datum)| (*attr, datum.to_vec()))
        .collect::<Vec<_>>();
    let values_owned = values
        .iter()
        .map(|(attr, datum)| (*attr, datum.to_vec()))
        .collect::<Vec<_>>();
    gen_codec::encode_relation_insert_request(&oid, table, &key_owned, &values_owned)
}

/// Decoded `relation-insert` request payload.
pub type RelationInsertRequest = (UniOid, String, Vec<(u64, Vec<u8>)>, Vec<(u64, Vec<u8>)>);

/// Decodes a `relation-insert` request frame into
/// `(oid, table, key, values)`.
pub fn decode_relation_insert_request(frame: &[u8]) -> RS<RelationInsertRequest> {
    decode_typed_frame(MessageKind::RelationInsert, frame)?;
    gen_codec::decode_relation_insert_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes a `relation-insert` result frame: body `[0, 0]` or
/// `[1, UniError]`.
pub fn encode_relation_insert_result(result: &RS<()>) -> Vec<u8> {
    gen_codec::encode_relation_insert_result(&rs_to_uni_unit(result))
}

/// Decodes a `relation-insert` result frame.
pub fn decode_relation_insert_result(frame: &[u8]) -> RS<()> {
    decode_typed_frame(MessageKind::RelationInsert, frame)?;
    from_gen(gen_codec::decode_relation_insert_result(frame))
}

// ---- fs-open ----

/// Encodes an `fs-open` request frame: body `[argv]`.
pub fn encode_fs_open_request(argv: &UniFsOpenArgv) -> Vec<u8> {
    gen_codec::encode_fs_open_request(argv)
}

/// Decodes an `fs-open` request frame into its argument record.
pub fn decode_fs_open_request(frame: &[u8]) -> RS<UniFsOpenArgv> {
    decode_typed_frame(MessageKind::FsOpen, frame)?;
    gen_codec::decode_fs_open_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes an `fs-open` result frame: body `[0, fd]` or `[1, UniError]`.
pub fn encode_fs_open_result(result: &RS<u32>) -> Vec<u8> {
    gen_codec::encode_fs_open_result_ref(&rs_to_uni(result))
}

/// Decodes an `fs-open` result frame into the new file descriptor.
pub fn decode_fs_open_result(frame: &[u8]) -> RS<u32> {
    decode_typed_frame(MessageKind::FsOpen, frame)?;
    from_gen(gen_codec::decode_fs_open_result(frame))
}

// ---- fs-close ----

/// Encodes an `fs-close` request frame: body `[fd]`.
pub fn encode_fs_close_request(fd: u32) -> Vec<u8> {
    gen_codec::encode_fs_close_request(fd)
}

/// Decodes an `fs-close` request frame into the file descriptor.
pub fn decode_fs_close_request(frame: &[u8]) -> RS<u32> {
    decode_typed_frame(MessageKind::FsClose, frame)?;
    gen_codec::decode_fs_close_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes an `fs-close` result frame: body `[0, 0]` or `[1, UniError]`.
pub fn encode_fs_close_result(result: &RS<()>) -> Vec<u8> {
    gen_codec::encode_fs_close_result(&rs_to_uni_unit(result))
}

/// Decodes an `fs-close` result frame.
pub fn decode_fs_close_result(frame: &[u8]) -> RS<()> {
    decode_typed_frame(MessageKind::FsClose, frame)?;
    from_gen(gen_codec::decode_fs_close_result(frame))
}

// ---- fs-read ----

/// Encodes an `fs-read` request frame: body `[fd, len]`.
pub fn encode_fs_read_request(fd: u32, len: u32) -> Vec<u8> {
    gen_codec::encode_fs_read_request(fd, len)
}

/// Decodes an `fs-read` request frame into `(fd, len)`.
pub fn decode_fs_read_request(frame: &[u8]) -> RS<(u32, u32)> {
    decode_typed_frame(MessageKind::FsRead, frame)?;
    gen_codec::decode_fs_read_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes an `fs-read` result frame: body `[0, data]` or `[1, UniError]`.
pub fn encode_fs_read_result(result: &RS<Vec<u8>>) -> Vec<u8> {
    gen_codec::encode_fs_read_result_ref(&rs_to_uni(result))
}

/// Decodes an `fs-read` result frame into the read bytes.
pub fn decode_fs_read_result(frame: &[u8]) -> RS<Vec<u8>> {
    decode_typed_frame(MessageKind::FsRead, frame)?;
    from_gen(gen_codec::decode_fs_read_result(frame))
}

// ---- fs-write ----

/// Encodes an `fs-write` request frame: body `[fd, data]`.
pub fn encode_fs_write_request(fd: u32, data: &[u8]) -> Vec<u8> {
    gen_codec::encode_fs_write_request(fd, data)
}

/// Decodes an `fs-write` request frame into `(fd, data)`.
pub fn decode_fs_write_request(frame: &[u8]) -> RS<(u32, Vec<u8>)> {
    decode_typed_frame(MessageKind::FsWrite, frame)?;
    gen_codec::decode_fs_write_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes an `fs-write` result frame: body `[0, n_written]` or
/// `[1, UniError]`.
pub fn encode_fs_write_result(result: &RS<u32>) -> Vec<u8> {
    gen_codec::encode_fs_write_result_ref(&rs_to_uni(result))
}

/// Decodes an `fs-write` result frame into the written byte count.
pub fn decode_fs_write_result(frame: &[u8]) -> RS<u32> {
    decode_typed_frame(MessageKind::FsWrite, frame)?;
    from_gen(gen_codec::decode_fs_write_result(frame))
}

// ---- fs-pread ----

/// Encodes an `fs-pread` request frame: body `[fd, offset, len]`.
pub fn encode_fs_pread_request(fd: u32, offset: u64, len: u32) -> Vec<u8> {
    gen_codec::encode_fs_pread_request(fd, offset, len)
}

/// Decodes an `fs-pread` request frame into `(fd, offset, len)`.
pub fn decode_fs_pread_request(frame: &[u8]) -> RS<(u32, u64, u32)> {
    decode_typed_frame(MessageKind::FsPread, frame)?;
    gen_codec::decode_fs_pread_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes an `fs-pread` result frame: body `[0, data]` or `[1, UniError]`.
pub fn encode_fs_pread_result(result: &RS<Vec<u8>>) -> Vec<u8> {
    gen_codec::encode_fs_pread_result_ref(&rs_to_uni(result))
}

/// Decodes an `fs-pread` result frame into the read bytes.
pub fn decode_fs_pread_result(frame: &[u8]) -> RS<Vec<u8>> {
    decode_typed_frame(MessageKind::FsPread, frame)?;
    from_gen(gen_codec::decode_fs_pread_result(frame))
}

// ---- fs-pwrite ----

/// Encodes an `fs-pwrite` request frame: body `[fd, offset, data]`.
pub fn encode_fs_pwrite_request(fd: u32, offset: u64, data: &[u8]) -> Vec<u8> {
    gen_codec::encode_fs_pwrite_request(fd, offset, data)
}

/// Decodes an `fs-pwrite` request frame into `(fd, offset, data)`.
pub fn decode_fs_pwrite_request(frame: &[u8]) -> RS<(u32, u64, Vec<u8>)> {
    decode_typed_frame(MessageKind::FsPwrite, frame)?;
    gen_codec::decode_fs_pwrite_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes an `fs-pwrite` result frame: body `[0, 0]` or `[1, UniError]`.
pub fn encode_fs_pwrite_result(result: &RS<()>) -> Vec<u8> {
    gen_codec::encode_fs_pwrite_result(&rs_to_uni_unit(result))
}

/// Decodes an `fs-pwrite` result frame.
pub fn decode_fs_pwrite_result(frame: &[u8]) -> RS<()> {
    decode_typed_frame(MessageKind::FsPwrite, frame)?;
    from_gen(gen_codec::decode_fs_pwrite_result(frame))
}

// ---- fs-lseek ----

/// Encodes an `fs-lseek` request frame: body `[fd, offset, whence]`.
pub fn encode_fs_lseek_request(fd: u32, offset: i64, whence: u32) -> Vec<u8> {
    gen_codec::encode_fs_lseek_request(fd, offset, whence)
}

/// Decodes an `fs-lseek` request frame into `(fd, offset, whence)`.
pub fn decode_fs_lseek_request(frame: &[u8]) -> RS<(u32, i64, u32)> {
    decode_typed_frame(MessageKind::FsLseek, frame)?;
    gen_codec::decode_fs_lseek_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes an `fs-lseek` result frame: body `[0, new_cursor]` or
/// `[1, UniError]`.
pub fn encode_fs_lseek_result(result: &RS<u64>) -> Vec<u8> {
    gen_codec::encode_fs_lseek_result_ref(&rs_to_uni(result))
}

/// Decodes an `fs-lseek` result frame into the new cursor position.
pub fn decode_fs_lseek_result(frame: &[u8]) -> RS<u64> {
    decode_typed_frame(MessageKind::FsLseek, frame)?;
    from_gen(gen_codec::decode_fs_lseek_result(frame))
}

// ---- fs-fstat ----

/// Encodes an `fs-fstat` request frame: body `[fd]`.
pub fn encode_fs_fstat_request(fd: u32) -> Vec<u8> {
    gen_codec::encode_fs_fstat_request(fd)
}

/// Decodes an `fs-fstat` request frame into the file descriptor.
pub fn decode_fs_fstat_request(frame: &[u8]) -> RS<u32> {
    decode_typed_frame(MessageKind::FsFstat, frame)?;
    gen_codec::decode_fs_fstat_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes an `fs-fstat` result frame: body `[0, UniFsStat]` or
/// `[1, UniError]`.
pub fn encode_fs_fstat_result(result: &RS<UniFsStat>) -> Vec<u8> {
    gen_codec::encode_fs_fstat_result_ref(&rs_to_uni(result))
}

/// Decodes an `fs-fstat` result frame into the stat record.
pub fn decode_fs_fstat_result(frame: &[u8]) -> RS<UniFsStat> {
    decode_typed_frame(MessageKind::FsFstat, frame)?;
    from_gen(gen_codec::decode_fs_fstat_result(frame))
}

// ---- fs-stat ----

/// Encodes an `fs-stat` request frame: body `[oid, path]`.
pub fn encode_fs_stat_request(oid: UniOid, path: &str) -> Vec<u8> {
    gen_codec::encode_fs_stat_request(&oid, path)
}

/// Decodes an `fs-stat` request frame into `(oid, path)`.
pub fn decode_fs_stat_request(frame: &[u8]) -> RS<(UniOid, String)> {
    decode_typed_frame(MessageKind::FsStat, frame)?;
    gen_codec::decode_fs_stat_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes an `fs-stat` result frame: body `[0, UniFsStat]` or
/// `[1, UniError]`.
pub fn encode_fs_stat_result(result: &RS<UniFsStat>) -> Vec<u8> {
    gen_codec::encode_fs_stat_result_ref(&rs_to_uni(result))
}

/// Decodes an `fs-stat` result frame into the stat record.
pub fn decode_fs_stat_result(frame: &[u8]) -> RS<UniFsStat> {
    decode_typed_frame(MessageKind::FsStat, frame)?;
    from_gen(gen_codec::decode_fs_stat_result(frame))
}

// ---- fs-fsync ----

/// Encodes an `fs-fsync` request frame: body `[fd]`.
pub fn encode_fs_fsync_request(fd: u32) -> Vec<u8> {
    gen_codec::encode_fs_fsync_request(fd)
}

/// Decodes an `fs-fsync` request frame into the file descriptor.
pub fn decode_fs_fsync_request(frame: &[u8]) -> RS<u32> {
    decode_typed_frame(MessageKind::FsFsync, frame)?;
    gen_codec::decode_fs_fsync_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes an `fs-fsync` result frame: body `[0, 0]` or `[1, UniError]`.
pub fn encode_fs_fsync_result(result: &RS<()>) -> Vec<u8> {
    gen_codec::encode_fs_fsync_result(&rs_to_uni_unit(result))
}

/// Decodes an `fs-fsync` result frame.
pub fn decode_fs_fsync_result(frame: &[u8]) -> RS<()> {
    decode_typed_frame(MessageKind::FsFsync, frame)?;
    from_gen(gen_codec::decode_fs_fsync_result(frame))
}

// ---- fs-readdir ----

/// Encodes an `fs-readdir` request frame: body `[oid, path]`.
pub fn encode_fs_readdir_request(oid: UniOid, path: &str) -> Vec<u8> {
    gen_codec::encode_fs_readdir_request(&oid, path)
}

/// Decodes an `fs-readdir` request frame into `(oid, path)`.
pub fn decode_fs_readdir_request(frame: &[u8]) -> RS<(UniOid, String)> {
    decode_typed_frame(MessageKind::FsReaddir, frame)?;
    gen_codec::decode_fs_readdir_request(frame).map_err(frame_error_to_mudu)
}

/// Encodes an `fs-readdir` result frame: body `[0, [UniFsDirent, ...]]` or
/// `[1, UniError]`.
pub fn encode_fs_readdir_result(result: &RS<Vec<UniFsDirent>>) -> Vec<u8> {
    gen_codec::encode_fs_readdir_result_ref(&rs_to_uni(result))
}

/// Decodes an `fs-readdir` result frame into the directory entries.
pub fn decode_fs_readdir_result(frame: &[u8]) -> RS<Vec<UniFsDirent>> {
    decode_typed_frame(MessageKind::FsReaddir, frame)?;
    from_gen(gen_codec::decode_fs_readdir_result(frame))
}
