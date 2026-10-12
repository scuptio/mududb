use super::*;

// Value-update helpers moved down to `storage` so relation storage can
// apply deltas under its row lock without depending on the server layer.
#[cfg(test)]
pub(crate) use ::storage::value_update::encode_sub_wrap_literal;
pub(crate) use ::storage::value_update::{apply_value_update, apply_value_update_with_deltas};
// Tuple-building helpers moved down to `storage`.
pub use ::storage::tuple_build::{build_key_tuple, build_value_tuple};

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

/// Project the columns named by `select` out of an encoded row: key columns
/// are decoded from `key`, value columns from `value`, and a NULL value
/// column yields `None`.
pub fn project_selected_fields(
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

/// Encodes a deferred delta assignment list as stored in `XLUpdate::delta`
/// (msgpack `[(attr, op_code, literal), ...]`).
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
            ::wal::xl_entry::TxOp::Commit(::wal::commit_ts::next_commit_timestamp()),
        ],
    }])
}

pub(crate) fn single_delete_batch(xid: u64, key: Vec<u8>) -> XLBatch {
    XLBatch::new(vec![::wal::xl_entry::XLEntry {
        xid,
        ops: vec![
            ::wal::xl_entry::TxOp::Begin,
            ::wal::xl_entry::TxOp::Write(::wal::xl_data_op::XLWrite::Delete(
                ::wal::xl_data_op::XLDelete {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key,
                },
            )),
            ::wal::xl_entry::TxOp::Commit(::wal::commit_ts::next_commit_timestamp()),
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
