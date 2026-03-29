pub mod async_;
#[cfg(all(not(target_arch = "wasm32"), feature = "standalone-adapter"))]
pub mod async_standalone;
#[cfg(all(target_arch = "wasm32", feature = "component-model", feature = "async"))]
pub mod async_wasm;
pub mod sync;
#[cfg(all(not(target_arch = "wasm32"), feature = "standalone-adapter"))]
pub mod sync_standalone;
#[cfg(all(
    target_arch = "wasm32",
    any(
        all(feature = "wasip1", not(feature = "component-model")),
        all(feature = "component-model", not(feature = "async"))
    )
))]
pub mod sync_wasm;

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::common::serde_utils::{deserialize_from, serialize_to_vec};
use mudu_binding::universal::uni_error::UniError;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_binding::universal::uni_result::UniResult;
use mudu_binding::universal::uni_result_set::UniResultSet;
use mudu_binding::universal::uni_tuple_row::UniTupleRow;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql::Context;
use mudu_contract::tuple::tuple_value::TupleValue;

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
        }),
    };
    serialize_to_vec(&payload)
}

pub(crate) fn result_batch_to_uni(batch: ResultBatch) -> RS<UniResultSet> {
    let cursor = serialize_to_vec(&UniOid::from(batch.oid()))?;
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
    let (oid, _) = deserialize_from::<UniOid>(cursor)?;
    Ok(oid.to_oid())
}
