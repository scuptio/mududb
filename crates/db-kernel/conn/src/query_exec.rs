//! Materialization of a query executor into rows plus a tuple descriptor.

use mudu::common::result::RS;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_contract::tuple::typed_bin::TypedBin;
use mudu_type::datum::DatumDyn;
use mudu_utils::task_trace;
use std::sync::Arc;

use ::catalog::contract::query_exec::QueryExec;

/// Drains a query executor into materialized rows plus the tuple
/// descriptor.
pub async fn query_exec_to_rows(exec: Arc<dyn QueryExec>) -> RS<(Vec<TupleValue>, TupleFieldDesc)> {
    let trace = task_trace!();
    trace.watch("query.exec.stage", "open");
    exec.open().await?;
    let desc = exec.tuple_desc()?;
    let mut rows = Vec::new();
    loop {
        trace.watch("query.exec.stage", "next");
        trace.watch("query.exec.row_index", &rows.len().to_string());
        let next = exec.next().await?;
        let Some(row) = next else {
            trace.watch("query.exec.stage", "done");
            break;
        };
        let value = {
            let _stage = ::catalog::stage_stats::StageGuard::new(
                ::catalog::stage_stats::Stage::ResultDecode,
            );
            tuple_field_to_value(row, &desc)?
        };
        rows.push(value);
    }
    Ok((rows, desc))
}

pub(crate) fn tuple_field_to_value(
    row: mudu_contract::tuple::tuple_field::TupleField,
    desc: &TupleFieldDesc,
) -> RS<TupleValue> {
    let mut values = Vec::with_capacity(row.fields().len());
    // Consume the row: each field's bytes move straight into the decoder
    // instead of being cloned out of the executor's tuple.
    for (index, field) in row.into_fields().into_iter().enumerate() {
        let datum_desc = &desc.fields()[index];
        match field {
            Some(field) => {
                let typed = TypedBin::new(datum_desc.type_family(), field);
                values.push(typed.to_value(datum_desc.data_type())?);
            }
            None => values.push(mudu_type::data_value::DataValue::null()),
        }
    }
    Ok(TupleValue::from(values))
}
