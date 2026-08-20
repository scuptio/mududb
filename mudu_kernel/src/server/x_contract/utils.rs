use super::*;
use mudu_type::data_type::DataType;
use mudu_type::data_type_function::{recv_binary, send_binary};
use mudu_type::data_value::DataValue;
use mudu_type::type_family::TypeFamily;

pub(crate) fn ensure_supported_predicate(predicate: &Predicate) -> RS<()> {
    match predicate {
        Predicate::CNF(items) | Predicate::DNF(items) if items.is_empty() => Ok(()),
        Predicate::KeyPrefixEq(_) => Ok(()),
        Predicate::CNF(items) | Predicate::DNF(items) => {
            let _ = items
                .iter()
                .flatten()
                .map(|(_oid, _filter): &(AttrIndex, Filter)| ())
                .count();
            Err(mudu_error!(
                ErrorCode::NotImplemented,
                "non-key predicates are not implemented in io_uring xcontract"
            ))
        }
    }
}

pub(crate) fn matches_predicate(
    desc: &TableDesc,
    key: &[u8],
    _value: &[u8],
    predicate: &Predicate,
) -> RS<bool> {
    match predicate {
        Predicate::CNF(items) | Predicate::DNF(items) if items.is_empty() => Ok(true),
        Predicate::KeyPrefixEq(prefix) => {
            for (attr, expected) in prefix {
                let field = desc.get_attr(*attr);
                let Some(primary_index) = field.primary_index() else {
                    return Ok(false);
                };
                let field_desc = desc.key_desc().get_field_desc(primary_index);
                let actual = field_desc.get(key)?;
                if actual != expected.as_slice() {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Predicate::CNF(_) | Predicate::DNF(_) => Err(mudu_error!(
            ErrorCode::NotImplemented,
            "non-key predicates are not implemented in io_uring xcontract"
        )),
    }
}

pub(crate) fn build_key_tuple(data: &VecDatum, desc: &TableDesc) -> RS<Vec<u8>> {
    build_tuple_for::<true>(data.data(), desc)
}

pub(crate) fn build_value_tuple(data: &VecDatum, desc: &TableDesc) -> RS<Vec<u8>> {
    build_tuple_for::<false>(data.data(), desc)
}

pub(crate) fn build_tuple_for<const IS_KEY: bool>(
    data: &Vec<(AttrIndex, DataBin)>,
    desc: &TableDesc,
) -> RS<Vec<u8>> {
    let mut vec_data = data.clone();
    let mut ok = true;
    vec_data.sort_by(|(id1, _), (id2, _)| {
        let (f1, f2) = (desc.get_attr(*id1), desc.get_attr(*id2));
        if f1.primary_index().is_some() != IS_KEY || f2.primary_index().is_some() != IS_KEY {
            ok = false;
        }
        f1.datum_index().cmp(&f2.datum_index())
    });
    if !ok {
        return Err(mudu_error!(ErrorCode::InvalidTuple));
    }
    let tuple_desc = if IS_KEY {
        desc.key_desc()
    } else {
        desc.value_desc()
    };
    let values: Vec<_> = vec_data.into_iter().map(|(_, v)| v).collect();
    if IS_KEY && tuple_desc.field_count() != values.len() {
        let expected_key_fields = desc
            .key_indices()
            .iter()
            .map(|index| desc.get_attr(*index).name().clone())
            .collect::<Vec<_>>();
        let provided_fields = data
            .iter()
            .map(|(attr, _)| {
                let field = desc.get_attr(*attr);
                format!(
                    "{}(column_index={}, datum_index={}, primary_index={:?})",
                    field.name(),
                    field.column_index(),
                    field.datum_index(),
                    field.primary_index()
                )
            })
            .collect::<Vec<_>>();
        return Err(mudu_error!(
            ErrorCode::InvalidTuple,
            format!(
                "build key tuple width mismatch for table {}: expected {} key fields {:?}, got {} provided fields {:?}",
                desc.name(),
                tuple_desc.field_count(),
                expected_key_fields,
                values.len(),
                provided_fields,
            )
        ));
    }
    if IS_KEY {
        return build_tuple(&values, tuple_desc);
    }

    let value_len = tuple_desc.field_count();
    let mut completed: Vec<Option<NullableValue>> = vec![None; value_len];
    for (attr, value) in data {
        let field = desc.get_attr(*attr);
        if field.primary_index().is_some() {
            return Err(mudu_error!(ErrorCode::InvalidTuple));
        }
        let datum_index = field.datum_index();
        if datum_index >= value_len || completed[datum_index].is_some() {
            return Err(mudu_error!(ErrorCode::InvalidTuple));
        }
        completed[datum_index] = Some(NullableValue::Value(
            field.type_desc().type_family().fn_recv()(value, field.type_desc())
                .map_err(|e| e.to_m_err())?
                .0,
        ));
    }
    for attr in desc.value_indices() {
        let field = desc.get_attr(*attr);
        let datum_index = field.datum_index();
        if completed[datum_index].is_some() {
            continue;
        }
        if field.nullable() {
            completed[datum_index] = Some(NullableValue::Null);
            continue;
        }
        let default = field.type_desc().type_family().fn_default()(field.type_desc())
            .map_err(|e| e.to_m_err())?;
        completed[datum_index] = Some(NullableValue::Value(default));
    }
    let completed = completed
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidTuple))?;
    TupleBuilder::new(tuple_desc).build(&completed)
}

pub(crate) fn build_bound_key(
    bound: &Bound<Vec<(AttrIndex, DataBin)>>,
    desc: &TableDesc,
) -> RS<Bound<Vec<u8>>> {
    match bound {
        Bound::Included(values) => {
            let tuple = build_key_tuple(&VecDatum::new(values.clone()), desc)?;
            Ok(Bound::Included(tuple))
        }
        Bound::Excluded(values) => {
            let tuple = build_key_tuple(&VecDatum::new(values.clone()), desc)?;
            Ok(Bound::Excluded(tuple))
        }
        Bound::Unbounded => Ok(Bound::Unbounded),
    }
}

pub(crate) fn bound_key_as_ref(bound: &Bound<Vec<u8>>) -> Bound<&[u8]> {
    match bound {
        Bound::Included(v) => Bound::Included(v.as_slice()),
        Bound::Excluded(v) => Bound::Excluded(v.as_slice()),
        Bound::Unbounded => Bound::Unbounded,
    }
}

pub(crate) fn rpc_bound_from_key_bound(
    bound: &Bound<Vec<(AttrIndex, DataBin)>>,
    desc: &TableDesc,
) -> RS<RpcBound> {
    match bound {
        Bound::Included(values) => Ok(RpcBound::Included(build_key_tuple(
            &VecDatum::new(values.clone()),
            desc,
        )?)),
        Bound::Excluded(values) => Ok(RpcBound::Excluded(build_key_tuple(
            &VecDatum::new(values.clone()),
            desc,
        )?)),
        Bound::Unbounded => Ok(RpcBound::Unbounded),
    }
}

pub(crate) fn rpc_bound_as_ref(bound: &RpcBound) -> Bound<&[u8]> {
    match bound {
        RpcBound::Included(bytes) => Bound::Included(bytes.as_slice()),
        RpcBound::Excluded(bytes) => Bound::Excluded(bytes.as_slice()),
        RpcBound::Unbounded => Bound::Unbounded,
    }
}

pub(crate) fn project_selected_fields(
    desc: &TableDesc,
    key: &[u8],
    value: &[u8],
    select: &VecSelTerm,
) -> RS<Vec<Option<DataBin>>> {
    let mut tuple_ret = Vec::with_capacity(select.vec().len());
    for i in select.vec() {
        let f = desc.get_attr(*i);
        let index = f.datum_index();
        let item = if f.primary_index().is_some() {
            let field_desc = desc.key_desc().get_field_desc(index);
            Some(field_desc.get(key)?.to_vec())
        } else {
            // Null-bitmap test only: the field bytes are extracted straight
            // from the stored tuple instead of decoding the whole datum just
            // to tell NULL from non-NULL.
            if mudu_contract::tuple::nullable_tuple::is_null(value, desc.value_desc(), index)? {
                None
            } else {
                let field_desc = desc.value_desc().get_field_desc(index);
                Some(field_desc.get(value)?.to_vec())
            }
        };
        tuple_ret.push(item);
    }
    Ok(tuple_ret)
}

pub(crate) fn apply_value_update(
    current: &TupleRaw,
    values: &VecDatum,
    desc: &TableDesc,
) -> RS<Vec<u8>> {
    let mut updated = current.clone();
    let mut data = values.data().clone();
    data.sort_by_key(|(attr, _)| desc.get_attr(*attr).datum_index());
    for (id, dat) in data.iter() {
        let field = desc.get_attr(*id);
        let mut delta = vec![];
        update_tuple(
            field.datum_index(),
            dat,
            desc.value_desc(),
            current,
            &mut delta,
        )?;
        for item in delta {
            item.apply_to(&mut updated);
        }
    }
    Ok(updated)
}

/// Apply an update that mixes absolute assignments with restricted expression
/// assignments (`SET col = col <+|-> <integer literal or ?>`).
///
/// Each delta assignment is evaluated against `current` — the latest
/// committed tuple read under the statement lock — producing an absolute
/// value that then flows through the regular [`apply_value_update`] path.
/// Evaluating on the locked row makes `col = col + 1` an atomic increment:
/// a concurrent transaction that committed after the caller's earlier plain
/// read cannot have its update lost.
pub(crate) fn apply_value_update_with_deltas(
    current: &TupleRaw,
    values: &VecDatum,
    deltas: &[DeltaAssign],
    desc: &TableDesc,
) -> RS<Vec<u8>> {
    if deltas.is_empty() {
        return apply_value_update(current, values, desc);
    }
    let mut resolved = values.data().clone();
    for assign in deltas {
        resolved.push((assign.attr, eval_delta_assignment(current, assign, desc)?));
    }
    apply_value_update(current, &VecDatum::new(resolved), desc)
}

/// Evaluate `col = col <+|-> operand` on the locked latest tuple, returning
/// the new absolute column value in the column's binary format.
fn eval_delta_assignment(
    current: &TupleRaw,
    assign: &DeltaAssign,
    desc: &TableDesc,
) -> RS<DataBin> {
    let field = desc.get_attr(assign.attr);
    let index = field.datum_index();
    let data_type = field.type_desc();
    let current_binary = match mudu_contract::tuple::nullable_tuple::read_value(
        &current.to_vec(),
        desc.value_desc(),
        index,
    )? {
        NullableValue::Null => {
            return Err(mudu_error!(
                ErrorCode::InvalidTuple,
                format!("arithmetic update on NULL column {}", field.name())
            ));
        }
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
        return eval_sub_wrap(&current_value, data_type, field.name(), &assign.literal);
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
    send_binary(&result, data_type).map_err(|e| e.to_m_err())
}

/// Encodes a deferred delta assignment list as stored in `XLUpdate::delta`
/// (msgpack `[(attr, op_code, literal), ...]`).
pub(crate) fn encode_delta_assigns(deltas: &[DeltaAssign]) -> RS<Vec<u8>> {
    let wire: Vec<(u64, u8, &[u8])> = deltas
        .iter()
        .map(|assign| {
            (
                assign.attr as u64,
                assign.op.op_code(),
                assign.literal.as_slice(),
            )
        })
        .collect();
    rmp_serde::to_vec(&wire)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode delta assignments error", e))
}

/// Decodes an `XLUpdate::delta` payload back into delta assignments.
pub(crate) fn decode_delta_assigns(delta: &[u8]) -> RS<Vec<DeltaAssign>> {
    let wire: Vec<(u64, u8, Vec<u8>)> = rmp_serde::from_slice(delta)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode delta assignments error", e))?;
    wire.into_iter()
        .map(|(attr, op, literal)| {
            Ok(DeltaAssign {
                attr: attr as mudu::common::id::AttrIndex,
                op: DeltaOp::from_op_code(op)?,
                literal,
            })
        })
        .collect()
}

/// Unpacks a conditional-restock literal (`[q, floor, wrap]`, three
/// big-endian i64s).
pub(crate) fn decode_sub_wrap_literal(literal: &[u8]) -> RS<(i64, i64, i64)> {
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
#[cfg(test)]
pub(crate) fn encode_sub_wrap_literal(q: i64, floor: i64, wrap: i64) -> Vec<u8> {
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

pub(crate) fn single_put_batch(xid: u64, key: Vec<u8>, value: Vec<u8>) -> XLBatch {
    XLBatch::new(vec![XLEntry {
        xid,
        ops: vec![
            TxOp::Begin,
            TxOp::Write(XLWrite::Insert(XLInsert {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key,
                value,
            })),
            crate::wal::xl_entry::TxOp::Commit,
        ],
    }])
}

pub(crate) fn single_delete_batch(xid: u64, key: Vec<u8>) -> XLBatch {
    XLBatch::new(vec![crate::wal::xl_entry::XLEntry {
        xid,
        ops: vec![
            crate::wal::xl_entry::TxOp::Begin,
            crate::wal::xl_entry::TxOp::Write(crate::wal::xl_data_op::XLWrite::Delete(
                crate::wal::xl_data_op::XLDelete {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key,
                },
            )),
            crate::wal::xl_entry::TxOp::Commit,
        ],
    }])
}

/// Collect the staged writes of `relation_id` that fall into the key range
/// `[start, end]` (bounds follow the semantics of `Bound`). This mirrors the
/// local range scan overlay so remote reads can apply read-your-writes.
pub(crate) fn staged_overlay_in_bounds(
    tx_mgr: &dyn TxMgr,
    relation_id: PhysicalRelationId,
    start: &Bound<Vec<u8>>,
    end: &Bound<Vec<u8>>,
) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
    let start_key = match start {
        Bound::Included(key) | Bound::Excluded(key) => key.clone(),
        Bound::Unbounded => Vec::new(),
    };
    let end_key = match end {
        Bound::Included(key) | Bound::Excluded(key) => key.clone(),
        Bound::Unbounded => Vec::new(),
    };
    // `staged_relation_items_in_range` matches `key >= start && key < end`
    // (empty end = unbounded), so the inclusive/exclusive boundary keys are
    // adjusted here.
    let mut items = tx_mgr.staged_relation_items_in_range(relation_id, &start_key, &end_key);
    if let Bound::Excluded(key) = start {
        items.retain(|(item_key, _)| item_key != key);
    }
    if let Bound::Included(key) = end {
        if let Some(value) = tx_mgr.get_relation(relation_id, key) {
            items.push((key.clone(), value));
        }
    }
    items
}

/// Acquire the commit locks for `write_ops`, waiting (with a bounded
/// timeout) while another commit holds any of them. Waiters hold no locks
/// while parked and acquisition stays atomic, so waiting cannot deadlock; a
/// timeout is the only failure and keeps the previous
/// "failed to acquire commit locks" behavior.
/// Bounded wait for contended statement/commit lock acquisition: long
/// enough to cover a lock holder's commit (prepare + apply), short enough to
/// fail genuine deadlocks instead of piling up latency.
pub(crate) const STATEMENT_LOCK_TIMEOUT: Duration = Duration::from_millis(5000);

/// Coordinator-scoped token identifying a transaction's statement-level
/// locks on remote owner workers. Coordinator worker ids are random OIDs, so
/// the xor with the (small) transaction xid stays unique in practice and
/// cannot collide with owner-local commit xids.
pub(crate) fn statement_lock_token(worker_id: OID, xid: u64) -> OID {
    worker_id ^ (xid as OID)
}

/// Acquire the commit locks for `write_ops`, waiting (with a bounded
/// timeout) while another commit holds any of them. Waiters hold no locks
/// while parked and acquisition stays atomic, so waiting cannot deadlock; a
/// timeout is the only failure and keeps the previous
/// "failed to acquire commit locks" behavior.
pub(crate) async fn acquire_commit_locks(
    lock_mgr: &XLockMgr,
    xid: OID,
    write_ops: &[(PhysicalRelationId, Vec<u8>)],
) -> RS<()> {
    if lock_mgr
        .lock_some(xid, write_ops, STATEMENT_LOCK_TIMEOUT)
        .await?
    {
        return Ok(());
    }
    Err(mudu_error!(
        ErrorCode::Transaction,
        format!("transaction {} failed to acquire commit locks", xid)
    ))
}

/// Return true when `tx` staged writes on at least one partition owned by a
/// worker other than `local_worker_id`. `partition_owners` maps each staged
/// partition id to its resolved owning worker; partitions without a resolved
/// owner are treated as locally owned.
pub(crate) fn is_cross_partition_tx(
    tx: &dyn TxMgr,
    local_worker_id: OID,
    partition_owners: &BTreeMap<OID, OID>,
) -> bool {
    tx.staged_relation_ops().keys().any(|relation_id| {
        partition_owners
            .get(&relation_id.partition_id)
            .copied()
            .unwrap_or(local_worker_id)
            != local_worker_id
    })
}
