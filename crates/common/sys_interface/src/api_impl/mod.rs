pub mod async_;
#[cfg(all(target_arch = "wasm32", feature = "component-model", feature = "async"))]
pub mod async_wasm;
pub mod sync;
#[cfg(all(
    target_arch = "wasm32",
    feature = "component-model",
    not(feature = "async")
))]
pub mod sync_wasm;

use mudu::common::id::OID;
use mudu::common::result::RS;

#[cfg(all(test, not(miri)))]
pub(crate) fn next_oid() -> OID {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::SeqCst) as OID
}
use mudu_binding::universal::mp_wire::{
    FromValue, ToValue, WireError, decode_value_prefix, encode_value,
};
use mudu_binding::universal::uni_error::UniError;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_binding::universal::uni_result::UniResult;
use mudu_binding::universal::uni_result_set::UniResultSet;
use mudu_binding::universal::uni_tuple_row::UniTupleRow;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql::Context;
use mudu_contract::tuple::tuple_value::TupleValue;

/// Maps a wire-runtime error to a `MuduError` with `ErrorCode::Decode`.
fn wire_err(e: WireError) -> mudu::error::MuduError {
    mudu::mudu_error!(mudu::error::ErrorCode::Decode, e.message)
}

pub(crate) fn drain_context_rows(context: &Context) -> RS<Vec<TupleValue>> {
    let mut rows = Vec::new();
    while let Some(row) = context.query_next()? {
        rows.push(row);
    }
    Ok(rows)
}

pub(crate) async fn drain_async_result_set(
    result_set: std::sync::Arc<dyn ResultSetAsync>,
) -> RS<Vec<TupleValue>> {
    let mut rows = Vec::new();
    while let Some(row) = result_set.next().await? {
        rows.push(row);
    }
    Ok(rows)
}

pub(crate) fn serialize_fetch_result(result: RS<ResultBatch>) -> RS<Vec<u8>> {
    let payload: UniResult<UniResultSet, UniError> = match result {
        Ok(batch) => UniResult::Ok(result_batch_to_uni(batch)?),
        Err(err) => UniResult::Err(UniError {
            err_code: err.ec().to_u32(),
            err_msg: err.message().to_string(),
            err_src: err.err_src().to_json_str(),
            err_loc: err.loc().to_string(),
            err_details: Vec::new(),
        }),
    };
    Ok(encode_value(&payload.to_value()))
}

pub(crate) fn result_batch_to_uni(batch: ResultBatch) -> RS<UniResultSet> {
    let cursor = encode_value(&UniOid::from(batch.oid()).to_value());
    let row_set = batch
        .into_rows()
        .into_iter()
        .map(UniTupleRow::uni_from)
        .collect::<RS<Vec<_>>>()?;
    Ok(UniResultSet {
        eof: true,
        row_set,
        cursor,
    })
}

pub(crate) fn fetch_cursor_oid(cursor: &[u8]) -> RS<OID> {
    // Trailing bytes are tolerated (the WIT ABI passes fixed-size buffers).
    let (value, _) = decode_value_prefix(cursor).map_err(wire_err)?;
    let oid = UniOid::from_value(&value).map_err(wire_err)?;
    Ok(oid.to_oid())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{fetch_cursor_oid, result_batch_to_uni, serialize_fetch_result};
    use mudu_binding::universal::mp_wire::{FromValue, ToValue, decode_value, encode_value};
    use mudu_binding::universal::uni_error::UniError;
    use mudu_binding::universal::uni_oid::UniOid;
    use mudu_binding::universal::uni_result::UniResult;
    use mudu_binding::universal::uni_result_set::UniResultSet;
    use mudu_contract::database::result_batch::ResultBatch;
    use mudu_contract::tuple::tuple_value::TupleValue;
    use mudu_type::data_value::DataValue;

    fn decode_fetch_payload(bytes: &[u8]) -> UniResult<UniResultSet, UniError> {
        UniResult::from_value(&decode_value(bytes).unwrap()).unwrap()
    }

    #[test]
    fn result_batch_helpers_roundtrip_cursor_and_rows() {
        let batch = ResultBatch::from(
            7,
            vec![TupleValue::from(vec![DataValue::from_i32(11)])],
            true,
        );
        let uni = result_batch_to_uni(batch).unwrap();

        assert!(uni.eof);
        assert_eq!(fetch_cursor_oid(&uni.cursor).unwrap(), 7);
        assert_eq!(uni.row_set.len(), 1);
    }

    #[test]
    fn serialize_fetch_result_encodes_ok_and_err_payloads() {
        let ok = serialize_fetch_result(Ok(ResultBatch::from(9, Vec::new(), true))).unwrap();
        match decode_fetch_payload(&ok) {
            UniResult::Ok(result_set) => {
                assert_eq!(fetch_cursor_oid(&result_set.cursor).unwrap(), 9)
            }
            UniResult::Err(err) => panic!("unexpected error payload: {}", err.err_msg),
        }

        let err = serialize_fetch_result(Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Parse,
            "boom"
        )))
        .unwrap();
        match decode_fetch_payload(&err) {
            UniResult::Ok(_) => panic!("expected error payload"),
            UniResult::Err(err) => assert_eq!(err.err_msg, "boom"),
        }
    }

    #[test]
    fn fetch_cursor_oid_decodes_universal_oid_binary() {
        let cursor = encode_value(&UniOid::from(42_u128).to_value());
        assert_eq!(fetch_cursor_oid(&cursor).unwrap(), 42);
    }
}
