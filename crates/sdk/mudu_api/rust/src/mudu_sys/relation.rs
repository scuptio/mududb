//! Frame codecs for the relation syscall family: `relation-get`,
//! `relation-update`, `relation-insert`.
//!
//! Thin adapter over the `mudu_gen`-generated codec in
//! [`super::generated::uni_syscall`]: key/value column lists are arrays of
//! `[attr, datum]` pairs with the datum as a MessagePack bin,
//! `relation-update` deltas are `[attr, op, datum]` triples, and results are
//! `[0, value]` / `[1, UniError]` (unit results as `[0, 0]`).

use super::generated::uni_syscall as gen_codec;
use super::payload;
use crate::error::ApiError;
use crate::types::UniReturn;
use crate::universal::uni_oid::UniOid;
use crate::universal::uni_relation::UniRelationDelta;

/// A key or value column of a relation row: `(attr, datum)` with the datum
/// in the column's binary encoding.
pub type RelationColumn = (u64, Vec<u8>);

/// Projected row returned by `relation-get`: one optional datum per selected
/// attribute, or `None` when no row matches the key.
pub type RelationRow = Option<Vec<Option<Vec<u8>>>>;

/// Serializes a `relation-get` request into a complete MSSP frame: body
/// `[oid, table, [[attr, datum], ...], [attr, ...]]`.
pub fn serialize_relation_get(
    oid: &UniOid,
    table: &str,
    key: &[RelationColumn],
    select: &[u64],
) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_relation_get_request(
        oid,
        table,
        &key.to_vec(),
        &select.to_vec(),
    ))
}

/// Deserializes a `relation-get` result MSSP frame into the optional
/// projected row.
pub fn deserialize_relation_get_result(bytes: &[u8]) -> Result<UniReturn<RelationRow>, ApiError> {
    gen_codec::decode_relation_get_result(bytes).map_err(payload::frame_error_to_api)
}

/// Serializes a `relation-update` request into a complete MSSP frame: body
/// `[oid, table, [[attr, datum], ...], [[attr, datum], ...],
/// [[attr, op, datum], ...]]`.
pub fn serialize_relation_update(
    oid: &UniOid,
    table: &str,
    key: &[RelationColumn],
    values: &[RelationColumn],
    deltas: &[UniRelationDelta],
) -> Result<Vec<u8>, ApiError> {
    let deltas = deltas
        .iter()
        .map(|delta| (delta.attr, delta.op, delta.datum.clone()))
        .collect();
    Ok(gen_codec::encode_relation_update_request(
        oid,
        table,
        &key.to_vec(),
        &values.to_vec(),
        &deltas,
    ))
}

/// Deserializes a `relation-update` result MSSP frame into the affected row
/// count.
pub fn deserialize_relation_update_result(bytes: &[u8]) -> Result<UniReturn<u64>, ApiError> {
    gen_codec::decode_relation_update_result(bytes).map_err(payload::frame_error_to_api)
}

/// Serializes a `relation-insert` request into a complete MSSP frame: body
/// `[oid, table, [[attr, datum], ...], [[attr, datum], ...]]`.
pub fn serialize_relation_insert(
    oid: &UniOid,
    table: &str,
    key: &[RelationColumn],
    values: &[RelationColumn],
) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_relation_insert_request(
        oid,
        table,
        &key.to_vec(),
        &values.to_vec(),
    ))
}

/// Deserializes a `relation-insert` result MSSP frame.
pub fn deserialize_relation_insert_result(bytes: &[u8]) -> Result<UniReturn<()>, ApiError> {
    gen_codec::decode_relation_insert_result(bytes).map_err(payload::frame_error_to_api)
}

/// Decoded `relation-get` request payload.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) type RelationGetRequest = (UniOid, String, Vec<RelationColumn>, Vec<u64>);

/// Decoded `relation-update` request payload.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) type RelationUpdateRequest = (
    UniOid,
    String,
    Vec<RelationColumn>,
    Vec<RelationColumn>,
    Vec<UniRelationDelta>,
);

/// Decoded `relation-insert` request payload.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) type RelationInsertRequest = (UniOid, String, Vec<RelationColumn>, Vec<RelationColumn>);

/// Decodes a `relation-get` request MSSP frame into
/// `(oid, table, key, select)`.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_relation_get_request(frame: &[u8]) -> Result<RelationGetRequest, ApiError> {
    gen_codec::decode_relation_get_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes a `relation-update` request MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_relation_update_request(
    frame: &[u8],
) -> Result<RelationUpdateRequest, ApiError> {
    let (oid, table, key, values, deltas) =
        gen_codec::decode_relation_update_request(frame).map_err(payload::frame_error_to_api)?;
    Ok((
        oid,
        table,
        key,
        values,
        deltas
            .into_iter()
            .map(|(attr, op, datum)| UniRelationDelta { attr, op, datum })
            .collect(),
    ))
}

/// Decodes a `relation-insert` request MSSP frame into
/// `(oid, table, key, values)`.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_relation_insert_request(
    frame: &[u8],
) -> Result<RelationInsertRequest, ApiError> {
    gen_codec::decode_relation_insert_request(frame).map_err(payload::frame_error_to_api)
}

/// Encodes a `relation-get` result (or error) into a complete MSSP frame:
/// the value arm is an array of datum-or-nil entries, or nil when no row
/// matches.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_relation_get_response(
    result: &UniReturn<RelationRow>,
) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_relation_get_result(result))
}

/// Encodes a `relation-update` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_relation_update_response(
    result: &UniReturn<u64>,
) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_relation_update_result(result))
}

/// Encodes a `relation-insert` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_relation_insert_response(result: &UniReturn<()>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_relation_insert_result(result))
}

/// Runs a `relation-get` syscall and returns the optional projected row.
pub async fn sys_relation_get(
    oid: &UniOid,
    table: &str,
    key: &[RelationColumn],
    select: &[u64],
) -> Result<UniReturn<RelationRow>, ApiError> {
    let request = serialize_relation_get(oid, table, key, select)?;
    let response = relation_get_raw(request).await?;
    deserialize_relation_get_result(&response)
}

/// Runs a `relation-update` syscall and returns the affected row count.
pub async fn sys_relation_update(
    oid: &UniOid,
    table: &str,
    key: &[RelationColumn],
    values: &[RelationColumn],
    deltas: &[UniRelationDelta],
) -> Result<UniReturn<u64>, ApiError> {
    let request = serialize_relation_update(oid, table, key, values, deltas)?;
    let response = relation_update_raw(request).await?;
    deserialize_relation_update_result(&response)
}

/// Runs a `relation-insert` syscall; duplicate keys fail.
pub async fn sys_relation_insert(
    oid: &UniOid,
    table: &str,
    key: &[RelationColumn],
    values: &[RelationColumn],
) -> Result<UniReturn<()>, ApiError> {
    let request = serialize_relation_insert(oid, table, key, values)?;
    let response = relation_insert_raw(request).await?;
    deserialize_relation_insert_result(&response)
}

#[allow(unused_variables)]
pub async fn relation_get_raw(relation_get_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    #[cfg(feature = "mock-sqlite")]
    {
        return crate::mock::MockSqliteMuduSysCall::relation_get_raw(relation_get_in).await;
    }

    #[cfg(all(
        target_arch = "wasm32",
        feature = "wasm-async",
        not(feature = "mock-sqlite")
    ))]
    {
        return super::wasm_async::relation_get_raw(relation_get_in).await;
    }

    #[allow(unreachable_code)]
    Err(ApiError::backend_unavailable(
        "no mudu backend configured; enable `mock-sqlite` or build wasm32 with `wasm-async`",
    ))
}

#[allow(unused_variables)]
pub async fn relation_update_raw(relation_update_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    #[cfg(feature = "mock-sqlite")]
    {
        return crate::mock::MockSqliteMuduSysCall::relation_update_raw(relation_update_in).await;
    }

    #[cfg(all(
        target_arch = "wasm32",
        feature = "wasm-async",
        not(feature = "mock-sqlite")
    ))]
    {
        return super::wasm_async::relation_update_raw(relation_update_in).await;
    }

    #[allow(unreachable_code)]
    Err(ApiError::backend_unavailable(
        "no mudu backend configured; enable `mock-sqlite` or build wasm32 with `wasm-async`",
    ))
}

#[allow(unused_variables)]
pub async fn relation_insert_raw(relation_insert_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    #[cfg(feature = "mock-sqlite")]
    {
        return crate::mock::MockSqliteMuduSysCall::relation_insert_raw(relation_insert_in).await;
    }

    #[cfg(all(
        target_arch = "wasm32",
        feature = "wasm-async",
        not(feature = "mock-sqlite")
    ))]
    {
        return super::wasm_async::relation_insert_raw(relation_insert_in).await;
    }

    #[allow(unreachable_code)]
    Err(ApiError::backend_unavailable(
        "no mudu backend configured; enable `mock-sqlite` or build wasm32 with `wasm-async`",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universal::uni_error::UniError;

    fn oid() -> UniOid {
        UniOid { h: 0, l: 7 }
    }

    #[test]
    fn relation_get_request_frame_matches_golden_bytes() {
        let key: Vec<RelationColumn> = vec![(1, vec![0x01]), (2, vec![0x02, 0x03])];
        let select: Vec<u64> = vec![3, 4];
        let frame = serialize_relation_get(&oid(), "t", &key, &select).unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x15, // kind RelationGet
            0x84, // {1: oid, 2: table, 3: key, 4: select}
            0x01, 0x82, 0x01, 0x00, 0x02, 0x07, // oid
            0x02, 0xA1, 0x74, // "t"
            0x03, 0x92, // 2 key columns
            0x92, 0x01, 0xC4, 0x01, 0x01, // [1, bin 01]
            0x92, 0x02, 0xC4, 0x02, 0x02, 0x03, // [2, bin 0203]
            0x04, 0x92, 0x03, 0x04, // select [3, 4]
        ];
        assert_eq!(frame, expected);

        let (decoded_oid, table, key, select) = decode_relation_get_request(&frame).unwrap();
        assert_eq!(decoded_oid.l, 7);
        assert_eq!(table, "t");
        assert_eq!(key, vec![(1, vec![0x01]), (2, vec![0x02, 0x03])]);
        assert_eq!(select, vec![3, 4]);
    }

    #[test]
    fn relation_update_request_frame_matches_golden_bytes() {
        let key: Vec<RelationColumn> = vec![(1, vec![0x01])];
        let values: Vec<RelationColumn> = vec![(2, vec![0x0A])];
        let deltas = vec![UniRelationDelta::add(3, vec![0x05])];
        let frame = serialize_relation_update(&oid(), "t", &key, &values, &deltas).unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x16, // kind RelationUpdate
            0x85, // {1: oid, 2: table, 3: key, 4: values, 5: deltas}
            0x01, 0x82, 0x01, 0x00, 0x02, 0x07, // oid
            0x02, 0xA1, 0x74, // "t"
            0x03, 0x91, 0x92, 0x01, 0xC4, 0x01, 0x01, // key [[1, bin 01]]
            0x04, 0x91, 0x92, 0x02, 0xC4, 0x01, 0x0A, // values [[2, bin 0A]]
            0x05, 0x91, 0x93, 0x03, 0x00, 0xC4, 0x01, 0x05, // deltas [[3, add, bin 05]]
        ];
        assert_eq!(frame, expected);

        let (decoded_oid, table, key, values, deltas) =
            decode_relation_update_request(&frame).unwrap();
        assert_eq!(decoded_oid.l, 7);
        assert_eq!(table, "t");
        assert_eq!(key, vec![(1, vec![0x01])]);
        assert_eq!(values, vec![(2, vec![0x0A])]);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].attr, 3);
        assert_eq!(
            deltas[0].op,
            crate::universal::uni_relation::RELATION_DELTA_OP_ADD
        );
        assert_eq!(deltas[0].datum, vec![0x05]);
    }

    #[test]
    fn relation_insert_request_frame_matches_golden_bytes() {
        let key: Vec<RelationColumn> = vec![(1, vec![0x01])];
        let values: Vec<RelationColumn> = vec![(2, vec![0x0A])];
        let frame = serialize_relation_insert(&oid(), "t", &key, &values).unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x17, // kind RelationInsert
            0x84, // {1: oid, 2: table, 3: key, 4: values}
            0x01, 0x82, 0x01, 0x00, 0x02, 0x07, // oid
            0x02, 0xA1, 0x74, // "t"
            0x03, 0x91, 0x92, 0x01, 0xC4, 0x01, 0x01, // key
            0x04, 0x91, 0x92, 0x02, 0xC4, 0x01, 0x0A, // values
        ];
        assert_eq!(frame, expected);

        let (decoded_oid, table, key, values) = decode_relation_insert_request(&frame).unwrap();
        assert_eq!(decoded_oid.l, 7);
        assert_eq!(table, "t");
        assert_eq!(key, vec![(1, vec![0x01])]);
        assert_eq!(values, vec![(2, vec![0x0A])]);
    }

    #[test]
    fn relation_get_result_frame_roundtrips_row_none_and_err() {
        let row: UniReturn<RelationRow> = Ok(Some(vec![Some(vec![0x0A]), None]));
        let frame = encode_relation_get_response(&row).unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x15, // kind RelationGet
            0x92, 0x00, // [0, row]
            0x92, 0xC4, 0x01, 0x0A, 0xC0, // [[bin 0A], nil]
        ];
        assert_eq!(frame, expected);
        match deserialize_relation_get_result(&frame).unwrap() {
            Ok(value) => assert_eq!(value, Some(vec![Some(vec![0x0A]), None])),
            Err(error) => panic!("unexpected error: {}", error.err_msg),
        }

        let miss: UniReturn<RelationRow> = Ok(None);
        let frame = encode_relation_get_response(&miss).unwrap();
        assert_eq!(&frame[16..], &[0x92, 0x00, 0xC0]);
        match deserialize_relation_get_result(&frame).unwrap() {
            Ok(value) => assert_eq!(value, None),
            Err(error) => panic!("unexpected error: {}", error.err_msg),
        }

        let err: UniReturn<RelationRow> = Err(UniError {
            err_code: 2,
            err_msg: "no table".to_string(),
            ..Default::default()
        });
        let frame = encode_relation_get_response(&err).unwrap();
        match deserialize_relation_get_result(&frame).unwrap() {
            Ok(_) => panic!("expected error variant"),
            Err(error) => assert_eq!(error.err_msg, "no table"),
        }
    }

    #[test]
    fn relation_update_result_frame_roundtrips_count_and_err() {
        let ok: UniReturn<u64> = Ok(1);
        let frame = encode_relation_update_response(&ok).unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x16, // kind RelationUpdate
            0x92, 0x00, 0x01, // body [0, 1]
        ];
        assert_eq!(frame, expected);
        match deserialize_relation_update_result(&frame).unwrap() {
            Ok(affected) => assert_eq!(affected, 1),
            Err(error) => panic!("unexpected error: {}", error.err_msg),
        }

        let frame = encode_relation_update_response(&Err(UniError {
            err_code: 1,
            err_msg: "conflict".to_string(),
            ..Default::default()
        }))
        .unwrap();
        match deserialize_relation_update_result(&frame).unwrap() {
            Ok(_) => panic!("expected error variant"),
            Err(error) => assert_eq!(error.err_msg, "conflict"),
        }
    }

    #[test]
    fn relation_insert_result_frame_roundtrips_unit_and_err() {
        let frame = encode_relation_insert_response(&Ok(())).unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x17, // kind RelationInsert
            0x92, 0x00, 0x00, // body [0, 0]
        ];
        assert_eq!(frame, expected);
        assert!(deserialize_relation_insert_result(&frame).unwrap().is_ok());

        let frame = encode_relation_insert_response(&Err(UniError {
            err_code: 1,
            err_msg: "duplicate key".to_string(),
            ..Default::default()
        }))
        .unwrap();
        match deserialize_relation_insert_result(&frame).unwrap() {
            Ok(()) => panic!("expected error variant"),
            Err(error) => assert_eq!(error.err_msg, "duplicate key"),
        }
    }
}
