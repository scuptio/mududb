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

use super::{
    Bin, BinRef, MessageKind, decode_request_frame, decode_result_frame, decode_unit_result_frame,
    encode_request_frame, encode_result_frame, encode_unit_result_frame, map_result_ref,
};
use crate::codec::adapter::{oid_from_mu, oid_to_mu};
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

// ---- query ----

/// Encodes a `query` request frame: body `[argv]`.
pub fn encode_query_request(argv: &UniQueryArgv) -> Vec<u8> {
    encode_request_frame(MessageKind::Query, &(argv,))
}

/// Decodes a `query` request frame into its argument record.
pub fn decode_query_request(frame: &[u8]) -> RS<UniQueryArgv> {
    let (argv,) = decode_request_frame(MessageKind::Query, frame)?;
    Ok(argv)
}

/// Encodes a `query` result frame: body `[0, UniQueryResult]` or
/// `[1, UniError]`.
pub fn encode_query_result(result: &RS<UniQueryResult>) -> Vec<u8> {
    encode_result_frame(MessageKind::Query, result)
}

/// Decodes a `query` result frame.
pub fn decode_query_result(frame: &[u8]) -> RS<UniQueryResult> {
    decode_result_frame(MessageKind::Query, frame)
}

// ---- command ----

/// Encodes a `command` request frame: body `[argv]`.
pub fn encode_command_request(argv: &UniCommandArgv) -> Vec<u8> {
    encode_request_frame(MessageKind::Command, &(argv,))
}

/// Decodes a `command` request frame into its argument record.
pub fn decode_command_request(frame: &[u8]) -> RS<UniCommandArgv> {
    let (argv,) = decode_request_frame(MessageKind::Command, frame)?;
    Ok(argv)
}

/// Encodes a `command` result frame: body `[0, UniCommandResult]` or
/// `[1, UniError]`.
pub fn encode_command_result(result: &RS<UniCommandResult>) -> Vec<u8> {
    encode_result_frame(MessageKind::Command, result)
}

/// Decodes a `command` result frame.
pub fn decode_command_result(frame: &[u8]) -> RS<UniCommandResult> {
    decode_result_frame(MessageKind::Command, frame)
}

// ---- batch ----

/// Encodes a `batch` request frame: body `[argv]`.
pub fn encode_batch_request(argv: &UniCommandArgv) -> Vec<u8> {
    encode_request_frame(MessageKind::Batch, &(argv,))
}

/// Decodes a `batch` request frame into its argument record.
pub fn decode_batch_request(frame: &[u8]) -> RS<UniCommandArgv> {
    let (argv,) = decode_request_frame(MessageKind::Batch, frame)?;
    Ok(argv)
}

/// Encodes a `batch` result frame: body `[0, UniCommandResult]` or
/// `[1, UniError]`.
pub fn encode_batch_result(result: &RS<UniCommandResult>) -> Vec<u8> {
    encode_result_frame(MessageKind::Batch, result)
}

/// Decodes a `batch` result frame.
pub fn decode_batch_result(frame: &[u8]) -> RS<UniCommandResult> {
    decode_result_frame(MessageKind::Batch, frame)
}

// ---- open-session ----

/// Encodes an `open-session` request frame: body `[worker_id]`.
pub fn encode_open_request(worker_id: UniOid) -> Vec<u8> {
    encode_request_frame(MessageKind::Open, &(worker_id,))
}

/// Decodes an `open-session` request frame into the worker OID.
pub fn decode_open_request(frame: &[u8]) -> RS<UniOid> {
    let (worker_id,) = decode_request_frame(MessageKind::Open, frame)?;
    Ok(worker_id)
}

/// Encodes an `open-session` result frame: body `[0, UniOid]` or
/// `[1, UniError]`.
pub fn encode_open_result(result: &RS<OID>) -> Vec<u8> {
    encode_result_frame(
        MessageKind::Open,
        &map_result_ref(result, |oid| oid_to_mu(*oid)),
    )
}

/// Decodes an `open-session` result frame into the new session OID.
pub fn decode_open_result(frame: &[u8]) -> RS<OID> {
    let oid = decode_result_frame::<UniOid>(MessageKind::Open, frame)?;
    Ok(oid_from_mu(oid))
}

// ---- close-session ----

/// Encodes a `close-session` request frame: body `[oid]`.
pub fn encode_close_request(oid: UniOid) -> Vec<u8> {
    encode_request_frame(MessageKind::Close, &(oid,))
}

/// Decodes a `close-session` request frame into the session OID.
pub fn decode_close_request(frame: &[u8]) -> RS<UniOid> {
    let (oid,) = decode_request_frame(MessageKind::Close, frame)?;
    Ok(oid)
}

/// Encodes a `close-session` result frame: body `[0, 0]` or `[1, UniError]`.
pub fn encode_close_result(result: &RS<()>) -> Vec<u8> {
    encode_unit_result_frame(MessageKind::Close, result)
}

/// Decodes a `close-session` result frame.
pub fn decode_close_result(frame: &[u8]) -> RS<()> {
    decode_unit_result_frame(MessageKind::Close, frame)
}

// ---- get ----

/// Encodes a `get` request frame: body `[oid, key]`.
pub fn encode_get_request(oid: UniOid, key: &[u8]) -> Vec<u8> {
    encode_request_frame(MessageKind::Get, &(oid, BinRef(key)))
}

/// Decodes a `get` request frame into `(oid, key)`.
pub fn decode_get_request(frame: &[u8]) -> RS<(UniOid, Vec<u8>)> {
    let (oid, Bin(key)) = decode_request_frame(MessageKind::Get, frame)?;
    Ok((oid, key))
}

/// Encodes a `get` result frame: body `[0, value-or-nil]` or `[1, UniError]`.
pub fn encode_get_result(result: &RS<Option<Vec<u8>>>) -> Vec<u8> {
    encode_result_frame(
        MessageKind::Get,
        &map_result_ref(result, |opt| opt.as_deref().map(BinRef)),
    )
}

/// Decodes a `get` result frame into the optional value.
pub fn decode_get_result(frame: &[u8]) -> RS<Option<Vec<u8>>> {
    let opt = decode_result_frame::<Option<Bin>>(MessageKind::Get, frame)?;
    Ok(opt.map(|Bin(value)| value))
}

// ---- put ----

/// Encodes a `put` request frame: body `[oid, key, value]`.
pub fn encode_put_request(oid: UniOid, key: &[u8], value: &[u8]) -> Vec<u8> {
    encode_request_frame(MessageKind::Put, &(oid, BinRef(key), BinRef(value)))
}

/// Decodes a `put` request frame into `(oid, key, value)`.
pub fn decode_put_request(frame: &[u8]) -> RS<(UniOid, Vec<u8>, Vec<u8>)> {
    let (oid, Bin(key), Bin(value)) = decode_request_frame(MessageKind::Put, frame)?;
    Ok((oid, key, value))
}

/// Encodes a `put` result frame: body `[0, 0]` or `[1, UniError]`.
pub fn encode_put_result(result: &RS<()>) -> Vec<u8> {
    encode_unit_result_frame(MessageKind::Put, result)
}

/// Decodes a `put` result frame.
pub fn decode_put_result(frame: &[u8]) -> RS<()> {
    decode_unit_result_frame(MessageKind::Put, frame)
}

// ---- delete ----

/// Encodes a `delete` request frame: body `[oid, key]`.
pub fn encode_delete_request(oid: UniOid, key: &[u8]) -> Vec<u8> {
    encode_request_frame(MessageKind::Delete, &(oid, BinRef(key)))
}

/// Decodes a `delete` request frame into `(oid, key)`.
pub fn decode_delete_request(frame: &[u8]) -> RS<(UniOid, Vec<u8>)> {
    let (oid, Bin(key)) = decode_request_frame(MessageKind::Delete, frame)?;
    Ok((oid, key))
}

/// Encodes a `delete` result frame: body `[0, 0]` or `[1, UniError]`.
pub fn encode_delete_result(result: &RS<()>) -> Vec<u8> {
    encode_unit_result_frame(MessageKind::Delete, result)
}

/// Decodes a `delete` result frame.
pub fn decode_delete_result(frame: &[u8]) -> RS<()> {
    decode_unit_result_frame(MessageKind::Delete, frame)
}

// ---- range ----

/// Encodes a `range` request frame: body `[oid, start, end]`.
pub fn encode_range_request(oid: UniOid, start: &[u8], end: &[u8]) -> Vec<u8> {
    encode_request_frame(MessageKind::Range, &(oid, BinRef(start), BinRef(end)))
}

/// Decodes a `range` request frame into `(oid, start, end)`.
pub fn decode_range_request(frame: &[u8]) -> RS<(UniOid, Vec<u8>, Vec<u8>)> {
    let (oid, Bin(start), Bin(end)) = decode_request_frame(MessageKind::Range, frame)?;
    Ok((oid, start, end))
}

/// Encodes a `range` result frame: body `[0, [[key, value], ...]]` or
/// `[1, UniError]`.
pub fn encode_range_result(result: &RS<Vec<(Vec<u8>, Vec<u8>)>>) -> Vec<u8> {
    encode_result_frame(
        MessageKind::Range,
        &map_result_ref(result, |items| {
            items
                .iter()
                .map(|(key, value)| (BinRef(key), BinRef(value)))
                .collect::<Vec<_>>()
        }),
    )
}

/// Decodes a `range` result frame into the key/value pairs.
pub fn decode_range_result(frame: &[u8]) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let items = decode_result_frame::<Vec<(Bin, Bin)>>(MessageKind::Range, frame)?;
    Ok(items
        .into_iter()
        .map(|(Bin(key), Bin(value))| (key, value))
        .collect())
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
    let key_refs = key
        .iter()
        .map(|(attr, datum)| (*attr, BinRef(datum)))
        .collect::<Vec<_>>();
    encode_request_frame(MessageKind::RelationGet, &(oid, table, key_refs, select))
}

/// Decoded `relation-get` request payload.
pub type RelationGetRequest = (UniOid, String, Vec<(u64, Vec<u8>)>, Vec<u64>);

/// Decodes a `relation-get` request frame into
/// `(oid, table, key, select)`.
pub fn decode_relation_get_request(frame: &[u8]) -> RS<RelationGetRequest> {
    let (oid, table, key, select) =
        decode_request_frame::<(UniOid, String, Vec<(u64, Bin)>, Vec<u64>)>(
            MessageKind::RelationGet,
            frame,
        )?;
    Ok((
        oid,
        table,
        key.into_iter()
            .map(|(attr, Bin(datum))| (attr, datum))
            .collect(),
        select,
    ))
}

/// Encodes a `relation-get` result frame: body
/// `[0, [[datum-or-nil], ...]-or-nil]` or `[1, UniError]`.
pub fn encode_relation_get_result(result: &RS<Option<Vec<Option<Vec<u8>>>>>) -> Vec<u8> {
    encode_result_frame(
        MessageKind::RelationGet,
        &map_result_ref(result, |opt| {
            opt.as_ref().map(|row| {
                row.iter()
                    .map(|field| field.as_deref().map(BinRef))
                    .collect::<Vec<_>>()
            })
        }),
    )
}

/// Decodes a `relation-get` result frame into the optional projected row.
pub fn decode_relation_get_result(frame: &[u8]) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    let opt = decode_result_frame::<Option<Vec<Option<Bin>>>>(MessageKind::RelationGet, frame)?;
    Ok(opt.map(|row| {
        row.into_iter()
            .map(|field| field.map(|Bin(datum)| datum))
            .collect()
    }))
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
    let key_refs = key
        .iter()
        .map(|(attr, datum)| (*attr, BinRef(datum)))
        .collect::<Vec<_>>();
    let value_refs = values
        .iter()
        .map(|(attr, datum)| (*attr, BinRef(datum)))
        .collect::<Vec<_>>();
    let delta_refs = deltas
        .iter()
        .map(|(attr, op, datum)| (*attr, *op, BinRef(datum)))
        .collect::<Vec<_>>();
    encode_request_frame(
        MessageKind::RelationUpdate,
        &(oid, table, key_refs, value_refs, delta_refs),
    )
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
    let (oid, table, key, values, deltas) = decode_request_frame::<(
        UniOid,
        String,
        Vec<(u64, Bin)>,
        Vec<(u64, Bin)>,
        Vec<(u64, u8, Bin)>,
    )>(MessageKind::RelationUpdate, frame)?;
    Ok((
        oid,
        table,
        key.into_iter()
            .map(|(attr, Bin(datum))| (attr, datum))
            .collect(),
        values
            .into_iter()
            .map(|(attr, Bin(datum))| (attr, datum))
            .collect(),
        deltas
            .into_iter()
            .map(|(attr, op, Bin(datum))| (attr, op, datum))
            .collect(),
    ))
}

/// Encodes a `relation-update` result frame: body `[0, affected]` or
/// `[1, UniError]`.
pub fn encode_relation_update_result(result: &RS<u64>) -> Vec<u8> {
    encode_result_frame(MessageKind::RelationUpdate, result)
}

/// Decodes a `relation-update` result frame into the affected row count.
pub fn decode_relation_update_result(frame: &[u8]) -> RS<u64> {
    decode_result_frame(MessageKind::RelationUpdate, frame)
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
    let key_refs = key
        .iter()
        .map(|(attr, datum)| (*attr, BinRef(datum)))
        .collect::<Vec<_>>();
    let value_refs = values
        .iter()
        .map(|(attr, datum)| (*attr, BinRef(datum)))
        .collect::<Vec<_>>();
    encode_request_frame(
        MessageKind::RelationInsert,
        &(oid, table, key_refs, value_refs),
    )
}

/// Decoded `relation-insert` request payload.
pub type RelationInsertRequest = (UniOid, String, Vec<(u64, Vec<u8>)>, Vec<(u64, Vec<u8>)>);

/// Decodes a `relation-insert` request frame into
/// `(oid, table, key, values)`.
pub fn decode_relation_insert_request(frame: &[u8]) -> RS<RelationInsertRequest> {
    let (oid, table, key, values) =
        decode_request_frame::<(UniOid, String, Vec<(u64, Bin)>, Vec<(u64, Bin)>)>(
            MessageKind::RelationInsert,
            frame,
        )?;
    Ok((
        oid,
        table,
        key.into_iter()
            .map(|(attr, Bin(datum))| (attr, datum))
            .collect(),
        values
            .into_iter()
            .map(|(attr, Bin(datum))| (attr, datum))
            .collect(),
    ))
}

/// Encodes a `relation-insert` result frame: body `[0, 0]` or
/// `[1, UniError]`.
pub fn encode_relation_insert_result(result: &RS<()>) -> Vec<u8> {
    encode_unit_result_frame(MessageKind::RelationInsert, result)
}

/// Decodes a `relation-insert` result frame.
pub fn decode_relation_insert_result(frame: &[u8]) -> RS<()> {
    decode_unit_result_frame(MessageKind::RelationInsert, frame)
}

// ---- fs-open ----

/// Encodes an `fs-open` request frame: body `[argv]`.
pub fn encode_fs_open_request(argv: &UniFsOpenArgv) -> Vec<u8> {
    encode_request_frame(MessageKind::FsOpen, &(argv,))
}

/// Decodes an `fs-open` request frame into its argument record.
pub fn decode_fs_open_request(frame: &[u8]) -> RS<UniFsOpenArgv> {
    let (argv,) = decode_request_frame(MessageKind::FsOpen, frame)?;
    Ok(argv)
}

/// Encodes an `fs-open` result frame: body `[0, fd]` or `[1, UniError]`.
pub fn encode_fs_open_result(result: &RS<u32>) -> Vec<u8> {
    encode_result_frame(MessageKind::FsOpen, result)
}

/// Decodes an `fs-open` result frame into the new file descriptor.
pub fn decode_fs_open_result(frame: &[u8]) -> RS<u32> {
    decode_result_frame(MessageKind::FsOpen, frame)
}

// ---- fs-close ----

/// Encodes an `fs-close` request frame: body `[fd]`.
pub fn encode_fs_close_request(fd: u32) -> Vec<u8> {
    encode_request_frame(MessageKind::FsClose, &(fd,))
}

/// Decodes an `fs-close` request frame into the file descriptor.
pub fn decode_fs_close_request(frame: &[u8]) -> RS<u32> {
    let (fd,) = decode_request_frame(MessageKind::FsClose, frame)?;
    Ok(fd)
}

/// Encodes an `fs-close` result frame: body `[0, 0]` or `[1, UniError]`.
pub fn encode_fs_close_result(result: &RS<()>) -> Vec<u8> {
    encode_unit_result_frame(MessageKind::FsClose, result)
}

/// Decodes an `fs-close` result frame.
pub fn decode_fs_close_result(frame: &[u8]) -> RS<()> {
    decode_unit_result_frame(MessageKind::FsClose, frame)
}

// ---- fs-read ----

/// Encodes an `fs-read` request frame: body `[fd, len]`.
pub fn encode_fs_read_request(fd: u32, len: u32) -> Vec<u8> {
    encode_request_frame(MessageKind::FsRead, &(fd, len))
}

/// Decodes an `fs-read` request frame into `(fd, len)`.
pub fn decode_fs_read_request(frame: &[u8]) -> RS<(u32, u32)> {
    decode_request_frame(MessageKind::FsRead, frame)
}

/// Encodes an `fs-read` result frame: body `[0, data]` or `[1, UniError]`.
pub fn encode_fs_read_result(result: &RS<Vec<u8>>) -> Vec<u8> {
    encode_result_frame(
        MessageKind::FsRead,
        &map_result_ref(result, |data| BinRef(data)),
    )
}

/// Decodes an `fs-read` result frame into the read bytes.
pub fn decode_fs_read_result(frame: &[u8]) -> RS<Vec<u8>> {
    let Bin(data) = decode_result_frame::<Bin>(MessageKind::FsRead, frame)?;
    Ok(data)
}

// ---- fs-write ----

/// Encodes an `fs-write` request frame: body `[fd, data]`.
pub fn encode_fs_write_request(fd: u32, data: &[u8]) -> Vec<u8> {
    encode_request_frame(MessageKind::FsWrite, &(fd, BinRef(data)))
}

/// Decodes an `fs-write` request frame into `(fd, data)`.
pub fn decode_fs_write_request(frame: &[u8]) -> RS<(u32, Vec<u8>)> {
    let (fd, Bin(data)) = decode_request_frame(MessageKind::FsWrite, frame)?;
    Ok((fd, data))
}

/// Encodes an `fs-write` result frame: body `[0, n_written]` or
/// `[1, UniError]`.
pub fn encode_fs_write_result(result: &RS<u32>) -> Vec<u8> {
    encode_result_frame(MessageKind::FsWrite, result)
}

/// Decodes an `fs-write` result frame into the written byte count.
pub fn decode_fs_write_result(frame: &[u8]) -> RS<u32> {
    decode_result_frame(MessageKind::FsWrite, frame)
}

// ---- fs-pread ----

/// Encodes an `fs-pread` request frame: body `[fd, offset, len]`.
pub fn encode_fs_pread_request(fd: u32, offset: u64, len: u32) -> Vec<u8> {
    encode_request_frame(MessageKind::FsPread, &(fd, offset, len))
}

/// Decodes an `fs-pread` request frame into `(fd, offset, len)`.
pub fn decode_fs_pread_request(frame: &[u8]) -> RS<(u32, u64, u32)> {
    decode_request_frame(MessageKind::FsPread, frame)
}

/// Encodes an `fs-pread` result frame: body `[0, data]` or `[1, UniError]`.
pub fn encode_fs_pread_result(result: &RS<Vec<u8>>) -> Vec<u8> {
    encode_result_frame(
        MessageKind::FsPread,
        &map_result_ref(result, |data| BinRef(data)),
    )
}

/// Decodes an `fs-pread` result frame into the read bytes.
pub fn decode_fs_pread_result(frame: &[u8]) -> RS<Vec<u8>> {
    let Bin(data) = decode_result_frame::<Bin>(MessageKind::FsPread, frame)?;
    Ok(data)
}

// ---- fs-pwrite ----

/// Encodes an `fs-pwrite` request frame: body `[fd, offset, data]`.
pub fn encode_fs_pwrite_request(fd: u32, offset: u64, data: &[u8]) -> Vec<u8> {
    encode_request_frame(MessageKind::FsPwrite, &(fd, offset, BinRef(data)))
}

/// Decodes an `fs-pwrite` request frame into `(fd, offset, data)`.
pub fn decode_fs_pwrite_request(frame: &[u8]) -> RS<(u32, u64, Vec<u8>)> {
    let (fd, offset, Bin(data)) = decode_request_frame(MessageKind::FsPwrite, frame)?;
    Ok((fd, offset, data))
}

/// Encodes an `fs-pwrite` result frame: body `[0, 0]` or `[1, UniError]`.
pub fn encode_fs_pwrite_result(result: &RS<()>) -> Vec<u8> {
    encode_unit_result_frame(MessageKind::FsPwrite, result)
}

/// Decodes an `fs-pwrite` result frame.
pub fn decode_fs_pwrite_result(frame: &[u8]) -> RS<()> {
    decode_unit_result_frame(MessageKind::FsPwrite, frame)
}

// ---- fs-lseek ----

/// Encodes an `fs-lseek` request frame: body `[fd, offset, whence]`.
pub fn encode_fs_lseek_request(fd: u32, offset: i64, whence: u32) -> Vec<u8> {
    encode_request_frame(MessageKind::FsLseek, &(fd, offset, whence))
}

/// Decodes an `fs-lseek` request frame into `(fd, offset, whence)`.
pub fn decode_fs_lseek_request(frame: &[u8]) -> RS<(u32, i64, u32)> {
    decode_request_frame(MessageKind::FsLseek, frame)
}

/// Encodes an `fs-lseek` result frame: body `[0, new_cursor]` or
/// `[1, UniError]`.
pub fn encode_fs_lseek_result(result: &RS<u64>) -> Vec<u8> {
    encode_result_frame(MessageKind::FsLseek, result)
}

/// Decodes an `fs-lseek` result frame into the new cursor position.
pub fn decode_fs_lseek_result(frame: &[u8]) -> RS<u64> {
    decode_result_frame(MessageKind::FsLseek, frame)
}

// ---- fs-fstat ----

/// Encodes an `fs-fstat` request frame: body `[fd]`.
pub fn encode_fs_fstat_request(fd: u32) -> Vec<u8> {
    encode_request_frame(MessageKind::FsFstat, &(fd,))
}

/// Decodes an `fs-fstat` request frame into the file descriptor.
pub fn decode_fs_fstat_request(frame: &[u8]) -> RS<u32> {
    let (fd,) = decode_request_frame(MessageKind::FsFstat, frame)?;
    Ok(fd)
}

/// Encodes an `fs-fstat` result frame: body `[0, UniFsStat]` or
/// `[1, UniError]`.
pub fn encode_fs_fstat_result(result: &RS<UniFsStat>) -> Vec<u8> {
    encode_result_frame(MessageKind::FsFstat, result)
}

/// Decodes an `fs-fstat` result frame into the stat record.
pub fn decode_fs_fstat_result(frame: &[u8]) -> RS<UniFsStat> {
    decode_result_frame(MessageKind::FsFstat, frame)
}

// ---- fs-stat ----

/// Encodes an `fs-stat` request frame: body `[oid, path]`.
pub fn encode_fs_stat_request(oid: UniOid, path: &str) -> Vec<u8> {
    encode_request_frame(MessageKind::FsStat, &(oid, path))
}

/// Decodes an `fs-stat` request frame into `(oid, path)`.
pub fn decode_fs_stat_request(frame: &[u8]) -> RS<(UniOid, String)> {
    decode_request_frame(MessageKind::FsStat, frame)
}

/// Encodes an `fs-stat` result frame: body `[0, UniFsStat]` or
/// `[1, UniError]`.
pub fn encode_fs_stat_result(result: &RS<UniFsStat>) -> Vec<u8> {
    encode_result_frame(MessageKind::FsStat, result)
}

/// Decodes an `fs-stat` result frame into the stat record.
pub fn decode_fs_stat_result(frame: &[u8]) -> RS<UniFsStat> {
    decode_result_frame(MessageKind::FsStat, frame)
}

// ---- fs-fsync ----

/// Encodes an `fs-fsync` request frame: body `[fd]`.
pub fn encode_fs_fsync_request(fd: u32) -> Vec<u8> {
    encode_request_frame(MessageKind::FsFsync, &(fd,))
}

/// Decodes an `fs-fsync` request frame into the file descriptor.
pub fn decode_fs_fsync_request(frame: &[u8]) -> RS<u32> {
    let (fd,) = decode_request_frame(MessageKind::FsFsync, frame)?;
    Ok(fd)
}

/// Encodes an `fs-fsync` result frame: body `[0, 0]` or `[1, UniError]`.
pub fn encode_fs_fsync_result(result: &RS<()>) -> Vec<u8> {
    encode_unit_result_frame(MessageKind::FsFsync, result)
}

/// Decodes an `fs-fsync` result frame.
pub fn decode_fs_fsync_result(frame: &[u8]) -> RS<()> {
    decode_unit_result_frame(MessageKind::FsFsync, frame)
}

// ---- fs-readdir ----

/// Encodes an `fs-readdir` request frame: body `[oid, path]`.
pub fn encode_fs_readdir_request(oid: UniOid, path: &str) -> Vec<u8> {
    encode_request_frame(MessageKind::FsReaddir, &(oid, path))
}

/// Decodes an `fs-readdir` request frame into `(oid, path)`.
pub fn decode_fs_readdir_request(frame: &[u8]) -> RS<(UniOid, String)> {
    decode_request_frame(MessageKind::FsReaddir, frame)
}

/// Encodes an `fs-readdir` result frame: body `[0, [UniFsDirent, ...]]` or
/// `[1, UniError]`.
pub fn encode_fs_readdir_result(result: &RS<Vec<UniFsDirent>>) -> Vec<u8> {
    encode_result_frame(MessageKind::FsReaddir, result)
}

/// Decodes an `fs-readdir` result frame into the directory entries.
pub fn decode_fs_readdir_result(frame: &[u8]) -> RS<Vec<UniFsDirent>> {
    decode_result_frame(MessageKind::FsReaddir, frame)
}
