use mudu::common::endian::{read_u128, write_u128};
use mudu::common::id::OID;
use mudu::common::result::RS;
use crate::universal::uni_session_open_argv::UniSessionOpenArgv;
use std::mem::size_of;

fn write_u32_be(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn read_u32_be(input: &[u8], offset: &mut usize) -> RS<u32> {
    let end = *offset + size_of::<u32>();
    if end > input.len() {
        return Err(mudu::m_error!(
            mudu::error::ec::EC::DecodeErr,
            "unexpected end of buffer"
        ));
    }
    let value = u32::from_be_bytes(input[*offset..end].try_into().unwrap());
    *offset = end;
    Ok(value)
}

fn read_bytes(input: &[u8], offset: &mut usize, len: usize) -> RS<Vec<u8>> {
    let end = *offset + len;
    if end > input.len() {
        return Err(mudu::m_error!(
            mudu::error::ec::EC::DecodeErr,
            "unexpected end of buffer"
        ));
    }
    let bytes = input[*offset..end].to_vec();
    *offset = end;
    Ok(bytes)
}

pub fn serialize_get_param(key: &[u8]) -> Vec<u8> {
    serialize_session_get_param(0, key)
}

pub fn serialize_session_get_param(session_id: OID, key: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(size_of::<u128>() + size_of::<u32>() + key.len());
    let mut session_buf = [0u8; size_of::<u128>()];
    write_u128(&mut session_buf, session_id);
    output.extend_from_slice(&session_buf);
    write_u32_be(&mut output, key.len() as u32);
    output.extend_from_slice(key);
    output
}

pub fn deserialize_get_param(input: &[u8]) -> RS<Vec<u8>> {
    Ok(deserialize_session_get_param(input)?.1)
}

pub fn deserialize_session_get_param(input: &[u8]) -> RS<(OID, Vec<u8>)> {
    if input.len() < size_of::<u128>() {
        return Err(mudu::m_error!(
            mudu::error::ec::EC::DecodeErr,
            "unexpected end of buffer"
        ));
    }
    let mut offset = 0;
    let session_id = read_u128(&input[offset..offset + size_of::<u128>()]);
    offset += size_of::<u128>();
    let key_len = read_u32_be(input, &mut offset)? as usize;
    let key = read_bytes(input, &mut offset, key_len)?;
    Ok((session_id, key))
}

pub fn serialize_get_result(value: Option<&[u8]>) -> Vec<u8> {
    let mut output = Vec::new();
    match value {
        Some(value) => {
            output.push(1);
            write_u32_be(&mut output, value.len() as u32);
            output.extend_from_slice(value);
        }
        None => output.push(0),
    }
    output
}

pub fn deserialize_get_result(input: &[u8]) -> RS<Option<Vec<u8>>> {
    if input.is_empty() {
        return Err(mudu::m_error!(
            mudu::error::ec::EC::DecodeErr,
            "empty get result"
        ));
    }
    match input[0] {
        0 => Ok(None),
        1 => {
            let mut offset = 1;
            let value_len = read_u32_be(input, &mut offset)? as usize;
            Ok(Some(read_bytes(input, &mut offset, value_len)?))
        }
        _ => Err(mudu::m_error!(
            mudu::error::ec::EC::DecodeErr,
            "invalid get result tag"
        )),
    }
}

pub fn serialize_put_param(key: &[u8], value: &[u8]) -> Vec<u8> {
    serialize_session_put_param(0, key, value)
}

pub fn serialize_session_put_param(session_id: OID, key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut output =
        Vec::with_capacity(size_of::<u128>() + size_of::<u32>() * 2 + key.len() + value.len());
    let mut session_buf = [0u8; size_of::<u128>()];
    write_u128(&mut session_buf, session_id);
    output.extend_from_slice(&session_buf);
    write_u32_be(&mut output, key.len() as u32);
    output.extend_from_slice(key);
    write_u32_be(&mut output, value.len() as u32);
    output.extend_from_slice(value);
    output
}

pub fn deserialize_put_param(input: &[u8]) -> RS<(Vec<u8>, Vec<u8>)> {
    let (_, key, value) = deserialize_session_put_param(input)?;
    Ok((key, value))
}

pub fn deserialize_session_put_param(input: &[u8]) -> RS<(OID, Vec<u8>, Vec<u8>)> {
    if input.len() < size_of::<u128>() {
        return Err(mudu::m_error!(
            mudu::error::ec::EC::DecodeErr,
            "unexpected end of buffer"
        ));
    }
    let mut offset = 0;
    let session_id = read_u128(&input[offset..offset + size_of::<u128>()]);
    offset += size_of::<u128>();
    let key_len = read_u32_be(input, &mut offset)? as usize;
    let key = read_bytes(input, &mut offset, key_len)?;
    let value_len = read_u32_be(input, &mut offset)? as usize;
    let value = read_bytes(input, &mut offset, value_len)?;
    Ok((session_id, key, value))
}

pub fn serialize_put_result() -> Vec<u8> {
    vec![1]
}

pub fn deserialize_put_result(input: &[u8]) -> RS<()> {
    if input == [1] {
        Ok(())
    } else {
        Err(mudu::m_error!(
            mudu::error::ec::EC::DecodeErr,
            "invalid put result"
        ))
    }
}

pub fn serialize_range_param(start_key: &[u8], end_key: &[u8]) -> Vec<u8> {
    serialize_session_range_param(0, start_key, end_key)
}

pub fn serialize_session_range_param(session_id: OID, start_key: &[u8], end_key: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(
        size_of::<u128>() + size_of::<u32>() * 2 + start_key.len() + end_key.len(),
    );
    let mut session_buf = [0u8; size_of::<u128>()];
    write_u128(&mut session_buf, session_id);
    output.extend_from_slice(&session_buf);
    write_u32_be(&mut output, start_key.len() as u32);
    output.extend_from_slice(start_key);
    write_u32_be(&mut output, end_key.len() as u32);
    output.extend_from_slice(end_key);
    output
}

pub fn deserialize_range_param(input: &[u8]) -> RS<(Vec<u8>, Vec<u8>)> {
    let (_, start, end) = deserialize_session_range_param(input)?;
    Ok((start, end))
}

pub fn deserialize_session_range_param(input: &[u8]) -> RS<(OID, Vec<u8>, Vec<u8>)> {
    if input.len() < size_of::<u128>() {
        return Err(mudu::m_error!(
            mudu::error::ec::EC::DecodeErr,
            "unexpected end of buffer"
        ));
    }
    let mut offset = 0;
    let session_id = read_u128(&input[offset..offset + size_of::<u128>()]);
    offset += size_of::<u128>();
    let start_len = read_u32_be(input, &mut offset)? as usize;
    let start = read_bytes(input, &mut offset, start_len)?;
    let end_len = read_u32_be(input, &mut offset)? as usize;
    let end = read_bytes(input, &mut offset, end_len)?;
    Ok((session_id, start, end))
}

pub fn serialize_open_param() -> Vec<u8> {
    serialize_open_argv_param(&UniSessionOpenArgv::default())
}

pub fn serialize_open_argv_param(argv: &UniSessionOpenArgv) -> Vec<u8> {
    mudu::common::serde_utils::serialize_to_vec(argv)
        .unwrap_or_else(|e| panic!("serialize open argv error: {}", e))
}

pub fn deserialize_open_param(input: &[u8]) -> RS<UniSessionOpenArgv> {
    if input.is_empty() {
        return Ok(UniSessionOpenArgv::default());
    }
    let (argv, _) = mudu::common::serde_utils::deserialize_from::<UniSessionOpenArgv>(input)?;
    Ok(argv)
}

pub fn serialize_open_result(session_id: OID) -> Vec<u8> {
    let mut output = vec![0u8; size_of::<u128>()];
    write_u128(&mut output, session_id);
    output
}

pub fn deserialize_open_result(input: &[u8]) -> RS<OID> {
    if input.len() < size_of::<u128>() {
        return Err(mudu::m_error!(
            mudu::error::ec::EC::DecodeErr,
            "unexpected end of buffer"
        ));
    }
    Ok(read_u128(&input[..size_of::<u128>()]))
}

pub fn serialize_close_param(session_id: OID) -> Vec<u8> {
    let mut output = vec![0u8; size_of::<u128>()];
    write_u128(&mut output, session_id);
    output
}

pub fn deserialize_close_param(input: &[u8]) -> RS<OID> {
    if input.len() < size_of::<u128>() {
        return Err(mudu::m_error!(
            mudu::error::ec::EC::DecodeErr,
            "unexpected end of buffer"
        ));
    }
    Ok(read_u128(&input[..size_of::<u128>()]))
}

pub fn serialize_close_result() -> Vec<u8> {
    vec![1]
}

pub fn deserialize_close_result(input: &[u8]) -> RS<()> {
    deserialize_put_result(input)
}

pub fn serialize_range_result(items: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut output = Vec::new();
    write_u32_be(&mut output, items.len() as u32);
    for (key, value) in items {
        write_u32_be(&mut output, key.len() as u32);
        output.extend_from_slice(key);
        write_u32_be(&mut output, value.len() as u32);
        output.extend_from_slice(value);
    }
    output
}

pub fn deserialize_range_result(input: &[u8]) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let mut offset = 0;
    let count = read_u32_be(input, &mut offset)? as usize;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        let key_len = read_u32_be(input, &mut offset)? as usize;
        let key = read_bytes(input, &mut offset, key_len)?;
        let value_len = read_u32_be(input, &mut offset)? as usize;
        let value = read_bytes(input, &mut offset, value_len)?;
        items.push((key, value));
    }
    Ok(items)
}
