//! Low-level filesystem syscall serialization helpers.
//!
//! This module is mostly boilerplate encode/decode routines and therefore
//! exempted from the `missing_docs` lint.
//!
//! These hand-rolled frames are superseded by the SyscallPayload v1 codec
//! ([`crate::codec::syscall_payload`]); the module is kept until downstream
//! crates migrate, so internal cross-references to deprecated items are
//! expected and allowed.
#![allow(missing_docs)]
#![allow(deprecated)]

use crate::codec::handle_sys_session::{
    decode_error_result, read_bytes, read_u32_be, write_u32_be,
};
use mudu::common::endian::{read_u128, write_u128};
use mudu::common::id::OID;
use mudu::common::result::RS;
use std::mem::size_of;

#[derive(Debug, Clone, PartialEq, Eq)]
#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub struct FsOpenParam {
    pub session_id: OID,
    pub oid: OID,
    pub path: String,
    pub flags: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub struct FsStatFrame {
    pub oid: OID,
    pub generation: u64,
    pub entry: String,
    pub length: u64,
    pub state: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub struct FsDirEnt {
    pub name: String,
    pub is_dir: bool,
    pub length: u64,
}

fn write_oid(output: &mut Vec<u8>, oid: OID) {
    let mut buf = [0u8; size_of::<u128>()];
    write_u128(&mut buf, oid);
    output.extend_from_slice(&buf);
}

fn read_u128_be(input: &[u8], offset: &mut usize) -> RS<u128> {
    let end = *offset + size_of::<u128>();
    if end > input.len() {
        return Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Decode,
            "unexpected end of buffer"
        ));
    }
    let value = read_u128(&input[*offset..end]);
    *offset = end;
    Ok(value)
}

fn write_u64_be(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn read_u64_be(input: &[u8], offset: &mut usize) -> RS<u64> {
    let end = *offset + size_of::<u64>();
    if end > input.len() {
        return Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Decode,
            "unexpected end of buffer"
        ));
    }
    let bytes = input[*offset..end]
        .try_into()
        .map_err(|_| mudu::mudu_error!(mudu::error::ErrorCode::Decode, "invalid u64 bytes"))?;
    let value = u64::from_be_bytes(bytes);
    *offset = end;
    Ok(value)
}

fn write_i64_be(output: &mut Vec<u8>, value: i64) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn read_i64_be(input: &[u8], offset: &mut usize) -> RS<i64> {
    let end = *offset + size_of::<i64>();
    if end > input.len() {
        return Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Decode,
            "unexpected end of buffer"
        ));
    }
    let bytes = input[*offset..end]
        .try_into()
        .map_err(|_| mudu::mudu_error!(mudu::error::ErrorCode::Decode, "invalid i64 bytes"))?;
    let value = i64::from_be_bytes(bytes);
    *offset = end;
    Ok(value)
}

fn write_string(output: &mut Vec<u8>, value: &str) {
    write_u32_be(output, value.len() as u32);
    output.extend_from_slice(value.as_bytes());
}

fn read_string(input: &[u8], offset: &mut usize) -> RS<String> {
    let len = read_u32_be(input, offset)? as usize;
    let bytes = read_bytes(input, offset, len)?;
    String::from_utf8(bytes)
        .map_err(|_| mudu::mudu_error!(mudu::error::ErrorCode::Decode, "invalid utf-8 string"))
}

fn write_fs_stat_frame(output: &mut Vec<u8>, stat: &FsStatFrame) {
    write_oid(output, stat.oid);
    write_u64_be(output, stat.generation);
    write_string(output, &stat.entry);
    write_u64_be(output, stat.length);
    write_u32_be(output, stat.state);
}

fn read_fs_stat_frame(input: &[u8], offset: &mut usize) -> RS<FsStatFrame> {
    let oid = read_u128_be(input, offset)?;
    let generation = read_u64_be(input, offset)?;
    let entry = read_string(input, offset)?;
    let length = read_u64_be(input, offset)?;
    let state = read_u32_be(input, offset)?;
    Ok(FsStatFrame {
        oid,
        generation,
        entry,
        length,
        state,
    })
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_open_param(param: &FsOpenParam) -> Vec<u8> {
    let mut output =
        Vec::with_capacity(size_of::<u128>() * 2 + size_of::<u32>() * 2 + param.path.len());
    write_oid(&mut output, param.session_id);
    write_oid(&mut output, param.oid);
    write_string(&mut output, &param.path);
    write_u32_be(&mut output, param.flags);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_open_param(input: &[u8]) -> RS<FsOpenParam> {
    let mut offset = 0;
    let session_id = read_u128_be(input, &mut offset)?;
    let oid = read_u128_be(input, &mut offset)?;
    let path = read_string(input, &mut offset)?;
    let flags = read_u32_be(input, &mut offset)?;
    Ok(FsOpenParam {
        session_id,
        oid,
        path,
        flags,
    })
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_open_result(fd: u32) -> Vec<u8> {
    let mut output = Vec::with_capacity(size_of::<u32>());
    write_u32_be(&mut output, fd);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_open_result(input: &[u8]) -> RS<u32> {
    decode_error_result(input)?;
    let mut offset = 0;
    read_u32_be(input, &mut offset)
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_close_param(session_id: OID, fd: u32) -> Vec<u8> {
    let mut output = Vec::with_capacity(size_of::<u128>() + size_of::<u32>());
    write_oid(&mut output, session_id);
    write_u32_be(&mut output, fd);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_close_param(input: &[u8]) -> RS<(OID, u32)> {
    let mut offset = 0;
    let session_id = read_u128_be(input, &mut offset)?;
    let fd = read_u32_be(input, &mut offset)?;
    Ok((session_id, fd))
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_close_result() -> Vec<u8> {
    vec![1]
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_close_result(input: &[u8]) -> RS<()> {
    decode_error_result(input)?;
    if input == [1] {
        Ok(())
    } else {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Decode,
            "invalid fs close result"
        ))
    }
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_read_param(session_id: OID, fd: u32, len: u32) -> Vec<u8> {
    let mut output = Vec::with_capacity(size_of::<u128>() + size_of::<u32>() * 2);
    write_oid(&mut output, session_id);
    write_u32_be(&mut output, fd);
    write_u32_be(&mut output, len);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_read_param(input: &[u8]) -> RS<(OID, u32, u32)> {
    let mut offset = 0;
    let session_id = read_u128_be(input, &mut offset)?;
    let fd = read_u32_be(input, &mut offset)?;
    let len = read_u32_be(input, &mut offset)?;
    Ok((session_id, fd, len))
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_read_result(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(size_of::<u32>() + data.len());
    write_u32_be(&mut output, data.len() as u32);
    output.extend_from_slice(data);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_read_result(input: &[u8]) -> RS<Vec<u8>> {
    decode_error_result(input)?;
    let mut offset = 0;
    let data_len = read_u32_be(input, &mut offset)? as usize;
    read_bytes(input, &mut offset, data_len)
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_write_param(session_id: OID, fd: u32, data: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(size_of::<u128>() + size_of::<u32>() * 2 + data.len());
    write_oid(&mut output, session_id);
    write_u32_be(&mut output, fd);
    write_u32_be(&mut output, data.len() as u32);
    output.extend_from_slice(data);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_write_param(input: &[u8]) -> RS<(OID, u32, Vec<u8>)> {
    let mut offset = 0;
    let session_id = read_u128_be(input, &mut offset)?;
    let fd = read_u32_be(input, &mut offset)?;
    let data_len = read_u32_be(input, &mut offset)? as usize;
    let data = read_bytes(input, &mut offset, data_len)?;
    Ok((session_id, fd, data))
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_write_result(n_written: u32) -> Vec<u8> {
    let mut output = Vec::with_capacity(size_of::<u32>());
    write_u32_be(&mut output, n_written);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_write_result(input: &[u8]) -> RS<u32> {
    decode_error_result(input)?;
    let mut offset = 0;
    read_u32_be(input, &mut offset)
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_pread_param(session_id: OID, fd: u32, offset: u64, len: u32) -> Vec<u8> {
    let mut output = Vec::with_capacity(
        size_of::<u128>() + size_of::<u32>() + size_of::<u64>() + size_of::<u32>(),
    );
    write_oid(&mut output, session_id);
    write_u32_be(&mut output, fd);
    write_u64_be(&mut output, offset);
    write_u32_be(&mut output, len);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_pread_param(input: &[u8]) -> RS<(OID, u32, u64, u32)> {
    let mut offset = 0;
    let session_id = read_u128_be(input, &mut offset)?;
    let fd = read_u32_be(input, &mut offset)?;
    let position = read_u64_be(input, &mut offset)?;
    let len = read_u32_be(input, &mut offset)?;
    Ok((session_id, fd, position, len))
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_pread_result(data: &[u8]) -> Vec<u8> {
    serialize_fs_read_result(data)
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_pread_result(input: &[u8]) -> RS<Vec<u8>> {
    deserialize_fs_read_result(input)
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_pwrite_param(session_id: OID, fd: u32, offset: u64, data: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(
        size_of::<u128>() + size_of::<u32>() + size_of::<u64>() + size_of::<u32>() + data.len(),
    );
    write_oid(&mut output, session_id);
    write_u32_be(&mut output, fd);
    write_u64_be(&mut output, offset);
    write_u32_be(&mut output, data.len() as u32);
    output.extend_from_slice(data);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_pwrite_param(input: &[u8]) -> RS<(OID, u32, u64, Vec<u8>)> {
    let mut offset = 0;
    let session_id = read_u128_be(input, &mut offset)?;
    let fd = read_u32_be(input, &mut offset)?;
    let position = read_u64_be(input, &mut offset)?;
    let data_len = read_u32_be(input, &mut offset)? as usize;
    let data = read_bytes(input, &mut offset, data_len)?;
    Ok((session_id, fd, position, data))
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_pwrite_result() -> Vec<u8> {
    vec![1]
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_pwrite_result(input: &[u8]) -> RS<()> {
    decode_error_result(input)?;
    if input == [1] {
        Ok(())
    } else {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Decode,
            "invalid fs pwrite result"
        ))
    }
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_lseek_param(session_id: OID, fd: u32, offset: i64, whence: u32) -> Vec<u8> {
    let mut output = Vec::with_capacity(
        size_of::<u128>() + size_of::<u32>() + size_of::<i64>() + size_of::<u32>(),
    );
    write_oid(&mut output, session_id);
    write_u32_be(&mut output, fd);
    write_i64_be(&mut output, offset);
    write_u32_be(&mut output, whence);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_lseek_param(input: &[u8]) -> RS<(OID, u32, i64, u32)> {
    let mut offset = 0;
    let session_id = read_u128_be(input, &mut offset)?;
    let fd = read_u32_be(input, &mut offset)?;
    let position = read_i64_be(input, &mut offset)?;
    let whence = read_u32_be(input, &mut offset)?;
    Ok((session_id, fd, position, whence))
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_lseek_result(new_cursor: u64) -> Vec<u8> {
    let mut output = Vec::with_capacity(size_of::<u64>());
    write_u64_be(&mut output, new_cursor);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_lseek_result(input: &[u8]) -> RS<u64> {
    decode_error_result(input)?;
    let mut offset = 0;
    read_u64_be(input, &mut offset)
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_fstat_param(session_id: OID, fd: u32) -> Vec<u8> {
    let mut output = Vec::with_capacity(size_of::<u128>() + size_of::<u32>());
    write_oid(&mut output, session_id);
    write_u32_be(&mut output, fd);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_fstat_param(input: &[u8]) -> RS<(OID, u32)> {
    let mut offset = 0;
    let session_id = read_u128_be(input, &mut offset)?;
    let fd = read_u32_be(input, &mut offset)?;
    Ok((session_id, fd))
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_fstat_result(stat: &FsStatFrame) -> Vec<u8> {
    let mut output = Vec::new();
    write_fs_stat_frame(&mut output, stat);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_fstat_result(input: &[u8]) -> RS<FsStatFrame> {
    decode_error_result(input)?;
    let mut offset = 0;
    read_fs_stat_frame(input, &mut offset)
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_stat_param(session_id: OID, oid: OID, path: &str) -> Vec<u8> {
    let mut output = Vec::with_capacity(size_of::<u128>() * 2 + size_of::<u32>() + path.len());
    write_oid(&mut output, session_id);
    write_oid(&mut output, oid);
    write_string(&mut output, path);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_stat_param(input: &[u8]) -> RS<(OID, OID, String)> {
    let mut offset = 0;
    let session_id = read_u128_be(input, &mut offset)?;
    let oid = read_u128_be(input, &mut offset)?;
    let path = read_string(input, &mut offset)?;
    Ok((session_id, oid, path))
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_stat_result(stat: &FsStatFrame) -> Vec<u8> {
    let mut output = Vec::new();
    write_fs_stat_frame(&mut output, stat);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_stat_result(input: &[u8]) -> RS<FsStatFrame> {
    decode_error_result(input)?;
    let mut offset = 0;
    read_fs_stat_frame(input, &mut offset)
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_fsync_param(session_id: OID, fd: u32) -> Vec<u8> {
    let mut output = Vec::with_capacity(size_of::<u128>() + size_of::<u32>());
    write_oid(&mut output, session_id);
    write_u32_be(&mut output, fd);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_fsync_param(input: &[u8]) -> RS<(OID, u32)> {
    let mut offset = 0;
    let session_id = read_u128_be(input, &mut offset)?;
    let fd = read_u32_be(input, &mut offset)?;
    Ok((session_id, fd))
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_fsync_result() -> Vec<u8> {
    vec![1]
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_fsync_result(input: &[u8]) -> RS<()> {
    decode_error_result(input)?;
    if input == [1] {
        Ok(())
    } else {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Decode,
            "invalid fs fsync result"
        ))
    }
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_readdir_param(session_id: OID, oid: OID, path: &str) -> Vec<u8> {
    let mut output = Vec::with_capacity(size_of::<u128>() * 2 + size_of::<u32>() + path.len());
    write_oid(&mut output, session_id);
    write_oid(&mut output, oid);
    write_string(&mut output, path);
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_readdir_param(input: &[u8]) -> RS<(OID, OID, String)> {
    let mut offset = 0;
    let session_id = read_u128_be(input, &mut offset)?;
    let oid = read_u128_be(input, &mut offset)?;
    let path = read_string(input, &mut offset)?;
    Ok((session_id, oid, path))
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn serialize_fs_readdir_result(entries: &[FsDirEnt]) -> Vec<u8> {
    let mut output = Vec::new();
    write_u32_be(&mut output, entries.len() as u32);
    for entry in entries {
        write_string(&mut output, &entry.name);
        output.push(if entry.is_dir { 1 } else { 0 });
        write_u64_be(&mut output, entry.length);
    }
    output
}

#[deprecated(
    note = "superseded by the SyscallPayload v1 codec; use crate::codec::syscall_payload instead"
)]
pub fn deserialize_fs_readdir_result(input: &[u8]) -> RS<Vec<FsDirEnt>> {
    decode_error_result(input)?;
    let mut offset = 0;
    let count = read_u32_be(input, &mut offset)? as usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let name = read_string(input, &mut offset)?;
        let flag = read_bytes(input, &mut offset, 1)?;
        let is_dir = match flag[0] {
            0 => false,
            1 => true,
            _ => {
                return Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::Decode,
                    "invalid is-dir flag"
                ));
            }
        };
        let length = read_u64_be(input, &mut offset)?;
        entries.push(FsDirEnt {
            name,
            is_dir,
            length,
        });
    }
    Ok(entries)
}

#[cfg(test)]
#[path = "handle_sys_fs_test.rs"]
mod handle_sys_fs_test;
