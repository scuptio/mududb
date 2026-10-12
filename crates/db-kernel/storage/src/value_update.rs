//! Tuple value-update helpers shared by the storage and cross-contract layers.
//!
//! `apply_value_update` / `apply_value_update_with_deltas` materialize an
//! absolute or delta update against the currently-committed tuple into a new
//! value tuple. They are deliberately kept in `storage` (rather than the
//! server-side cross-contract layer) because relation storage applies deltas
//! under its row lock, so both layers depend on the same implementation.

use ::catalog::contract::table_desc::TableDesc;
use ::xengine::api::{DeltaAssign, DeltaOp, VecDatum};
use ::xengine::DataBin;
use mudu::common::id::AttrIndex;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::tuple::nullable_tuple::{NullableValue, TupleBuilder};
use mudu_contract::tuple::tuple_binary::TupleBinary as TupleRaw;
use mudu_contract::tuple::update_tuple::update_tuple;
use mudu_type::data_type::DataType;
use mudu_type::data_type_function::{recv_binary, send_binary};
use mudu_type::data_value::DataValue;
use mudu_type::type_family::TypeFamily;

/// Materializes an absolute update against the currently-committed value
/// tuple `current`, producing the new encoded value tuple. `values` holds the
/// assigned non-key columns and `null_assignments` the columns set to NULL.
///
/// Writes that assign NULL, or that write a currently-NULL column, cannot be
/// spliced in place and rebuild the whole tuple instead.
pub fn apply_value_update(
    current: &TupleRaw,
    values: &VecDatum,
    null_assignments: &[AttrIndex],
    desc: &TableDesc,
) -> RS<Vec<u8>> {
    let mut updated = current.clone();
    let mut data = values.data().clone();
    data.sort_by_key(|(attr, _)| desc.get_attr(*attr).datum_index());
    // NULL assignments and in-place writes of a currently-NULL field cannot
    // materialize through the delta-splice path (NULL stores a (0,0) slot
    // plus a set null-bitmap bit, and `update_tuple` never touches the
    // bitmap), so such updates rebuild the whole tuple instead.
    if !null_assignments.is_empty() {
        return rebuild_value_tuple(current, &data, null_assignments, desc);
    }
    for (id, _) in data.iter() {
        let field = desc.get_attr(*id);
        if mudu_contract::tuple::nullable_tuple::is_null(
            &updated,
            desc.value_desc(),
            field.datum_index(),
        )? {
            return rebuild_value_tuple(current, &data, null_assignments, desc);
        }
    }
    for (id, dat) in data.iter() {
        let field = desc.get_attr(*id);
        let mut delta = vec![];
        update_tuple(
            field.datum_index(),
            dat,
            desc.value_desc(),
            &updated,
            &mut delta,
        )?;
        for item in delta {
            item.apply_to(&mut updated);
        }
    }
    Ok(updated)
}

/// Rebuild the whole value tuple when an update writes NULL or writes a
/// currently-NULL column: decode every field from `current`, overlay the new
/// values (`null_assignments` columns become NULL), and build a fresh tuple
/// so the null bitmap and slots are rewritten correctly.
fn rebuild_value_tuple(
    current: &TupleRaw,
    data: &[(AttrIndex, DataBin)],
    null_assignments: &[AttrIndex],
    desc: &TableDesc,
) -> RS<Vec<u8>> {
    let tuple_desc = desc.value_desc();
    let mut completed: Vec<Option<NullableValue>> = vec![None; tuple_desc.field_count()];
    let mut overlay = |attr: AttrIndex, value: NullableValue| -> RS<()> {
        let field = desc.get_attr(attr);
        if field.primary_index().is_some() {
            return Err(mudu_error!(ErrorCode::InvalidTuple));
        }
        let datum_index = field.datum_index();
        if datum_index >= tuple_desc.field_count() || completed[datum_index].is_some() {
            return Err(mudu_error!(ErrorCode::InvalidTuple));
        }
        completed[datum_index] = Some(value);
        Ok(())
    };
    for (attr, value) in data {
        let field = desc.get_attr(*attr);
        overlay(
            *attr,
            NullableValue::Value(
                field.type_desc().type_family().fn_recv()(value, field.type_desc())
                    .map_err(|e| e.to_m_err())?
                    .0,
            ),
        )?;
    }
    for attr in null_assignments {
        overlay(*attr, NullableValue::Null)?;
    }
    for (index, item) in completed.iter_mut().enumerate() {
        if item.is_none() {
            *item = Some(mudu_contract::tuple::nullable_tuple::read_value(
                current, tuple_desc, index,
            )?);
        }
    }
    let completed = completed
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidTuple))?;
    TupleBuilder::new(tuple_desc).build(&completed)
}

/// Apply an update that mixes absolute assignments with restricted expression
/// assignments (`SET col = col <+|-> <integer literal or ?>`) and NULL
/// assignments (`SET col = NULL`).
///
/// Each delta assignment is evaluated against `current` — the latest
/// committed tuple read under the statement lock — producing an absolute
/// value that then flows through the regular [`apply_value_update`] path; a
/// delta over a currently-NULL column evaluates to NULL (per SQL semantics)
/// and joins the NULL assignments on the rebuild path. Evaluating on the
/// locked row makes `col = col + 1` an atomic increment: a concurrent
/// transaction that committed after the caller's earlier plain read cannot
/// have its update lost.
pub fn apply_value_update_with_deltas(
    current: &TupleRaw,
    values: &VecDatum,
    deltas: &[DeltaAssign],
    null_assignments: &[AttrIndex],
    desc: &TableDesc,
) -> RS<Vec<u8>> {
    if deltas.is_empty() {
        return apply_value_update(current, values, null_assignments, desc);
    }
    let mut resolved = values.data().clone();
    let mut null_assignments = null_assignments.to_vec();
    for assign in deltas {
        match eval_delta_assignment(current, assign, desc)? {
            Some(binary) => resolved.push((assign.attr, binary)),
            None => null_assignments.push(assign.attr),
        }
    }
    apply_value_update(current, &VecDatum::new(resolved), &null_assignments, desc)
}

/// Evaluate `col = col <+|-> operand` on the locked latest tuple, returning
/// the new absolute column value in the column's binary format, or `None`
/// when the result is SQL NULL (the column's current value is NULL).
fn eval_delta_assignment(
    current: &TupleRaw,
    assign: &DeltaAssign,
    desc: &TableDesc,
) -> RS<Option<DataBin>> {
    let field = desc.get_attr(assign.attr);
    let index = field.datum_index();
    let data_type = field.type_desc();
    let current_binary = match mudu_contract::tuple::nullable_tuple::read_value(
        &current.to_vec(),
        desc.value_desc(),
        index,
    )? {
        // NULL <+|-> operand is NULL per SQL semantics.
        NullableValue::Null => return Ok(None),
        NullableValue::Value(_) => desc
            .value_desc()
            .get_field_desc(index)
            .get(current)?
            .to_vec(),
    };
    let current_value = recv_binary(&current_binary, data_type).map_err(|e| e.to_m_err())?;
    let literal_value = recv_binary(&assign.literal, data_type).map_err(|e| e.to_m_err())?;

    // The conditional-restock op packs its own parameters
    // (`[q, floor, wrap]` as three big-endian i64s) instead of a column-typed
    // operand; it is handled on a separate path below.
    if assign.op == DeltaOp::SubWrapDeferred {
        return eval_sub_wrap(&current_value, data_type, field.name(), &assign.literal).map(Some);
    }

    /// Decode both operands as `$expect`, apply the checked operation, and
    /// wrap the result back with `$from`.
    macro_rules! int_delta {
        ($expect:ident, $from:ident) => {{
            let current = *current_value.$expect();
            let literal = *literal_value.$expect();
            let result = match assign.op {
                DeltaOp::Add | DeltaOp::AddDeferred => current.checked_add(literal),
                DeltaOp::Sub | DeltaOp::SubDeferred => current.checked_sub(literal),
                DeltaOp::SubWrapDeferred => unreachable!("handled above"),
            };
            result.map(DataValue::$from).ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidTuple,
                    format!(
                        "arithmetic update on column {} overflows the column type",
                        field.name()
                    )
                )
            })?
        }};
    }
    let result = match data_type.type_family() {
        TypeFamily::I32 => int_delta!(expect_i32, from_i32),
        TypeFamily::I64 => int_delta!(expect_i64, from_i64),
        TypeFamily::I128 => int_delta!(expect_i128, from_i128),
        TypeFamily::U128 => int_delta!(expect_u128, from_u128),
        // Numeric money columns (e.g. TPC-C `w_ytd NUMERIC(12,2)`) receive an
        // integer delta operand; BigDecimal arithmetic is exact and cannot
        // overflow the arbitrary-precision representation.
        TypeFamily::Numeric => {
            let current = current_value.expect_numeric();
            let literal = literal_value.expect_numeric();
            let result = match assign.op {
                DeltaOp::Add | DeltaOp::AddDeferred => {
                    current.as_bigdecimal() + literal.as_bigdecimal()
                }
                DeltaOp::Sub | DeltaOp::SubDeferred => {
                    current.as_bigdecimal() - literal.as_bigdecimal()
                }
                DeltaOp::SubWrapDeferred => unreachable!("handled above"),
            };
            DataValue::from_numeric(mudu::data_type::numeric::Numeric::from_bigdecimal(result))
        }
        other => {
            return Err(mudu_error!(
                ErrorCode::InvalidType,
                format!(
                    "arithmetic update requires an integer column, got {:?}",
                    other
                )
            ));
        }
    };
    send_binary(&result, data_type)
        .map(Some)
        .map_err(|e| e.to_m_err())
}
/// big-endian i64s).
fn decode_sub_wrap_literal(literal: &[u8]) -> RS<(i64, i64, i64)> {
    if literal.len() != 24 {
        return Err(mudu_error!(
            ErrorCode::Decode,
            format!(
                "conditional restock delta expects a 24-byte [q, floor, wrap] literal, got {}",
                literal.len()
            )
        ));
    }
    let read = |offset: usize| {
        i64::from_be_bytes(literal[offset..offset + 8].try_into().unwrap_or([0; 8]))
    };
    Ok((read(0), read(8), read(16)))
}

/// Packs a conditional-restock literal (`[q, floor, wrap]`, three big-endian
/// i64s). Test-only today: production callers pack the literal in the
/// relation-update binding (`UniRelationDelta::sub_wrap`).
pub fn encode_sub_wrap_literal(q: i64, floor: i64, wrap: i64) -> Vec<u8> {
    let mut out = Vec::with_capacity(24);
    out.extend_from_slice(&q.to_be_bytes());
    out.extend_from_slice(&floor.to_be_bytes());
    out.extend_from_slice(&wrap.to_be_bytes());
    out
}

/// Evaluate the conditional restock: `y = current - q; if y < floor { y + wrap }`.
/// With `wrap > floor` this is exactly `((current - floor - q) mod wrap) + floor`,
/// which makes any two such updates commute.
fn eval_sub_wrap(
    current_value: &DataValue,
    data_type: &DataType,
    field_name: &str,
    literal: &[u8],
) -> RS<DataBin> {
    let (q, floor, wrap) = decode_sub_wrap_literal(literal)?;
    if wrap <= floor {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("conditional restock requires wrap > floor, got wrap={wrap} floor={floor}")
        ));
    }
    let current_i64: i64 = match data_type.type_family() {
        TypeFamily::I32 => (*current_value.expect_i32()) as i64,
        TypeFamily::I64 => *current_value.expect_i64(),
        other => {
            return Err(mudu_error!(
                ErrorCode::InvalidType,
                format!(
                    "conditional restock requires an i32/i64 column, got {:?}",
                    other
                )
            ));
        }
    };
    let y = current_i64
        .checked_sub(q)
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidTuple, "restock delta overflows"))?;
    let result = if y < floor {
        y.checked_add(wrap)
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidTuple, "restock wrap overflows"))?
    } else {
        y
    };
    let result_value = match data_type.type_family() {
        TypeFamily::I32 => DataValue::from_i32(i32::try_from(result).map_err(|_| {
            mudu_error!(
                ErrorCode::InvalidTuple,
                format!("conditional restock on column {} overflows i32", field_name)
            )
        })?),
        TypeFamily::I64 => DataValue::from_i64(result),
        _ => unreachable!("checked above"),
    };
    send_binary(&result_value, data_type).map_err(|e| e.to_m_err())
}
