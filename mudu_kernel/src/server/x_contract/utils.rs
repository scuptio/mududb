use super::*;

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
    data: &Vec<(AttrIndex, DatBin)>,
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
            field.type_desc().dat_type_id().fn_recv()(value, field.type_desc())
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
        let default = field.type_desc().dat_type_id().fn_default()(field.type_desc())
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
    bound: &Bound<Vec<(AttrIndex, DatBin)>>,
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
    bound: &Bound<Vec<(AttrIndex, DatBin)>>,
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
) -> RS<Vec<Option<DatBin>>> {
    let mut tuple_ret = vec![];
    for i in select.vec() {
        let f = desc.get_attr(*i);
        let index = f.datum_index();
        let item = if f.primary_index().is_some() {
            let field_desc = desc.key_desc().get_field_desc(index);
            Some(field_desc.get(key)?.to_vec())
        } else {
            match mudu_contract::tuple::nullable_tuple::read_value(
                &value.to_vec(),
                desc.value_desc(),
                index,
            )? {
                NullableValue::Null => None,
                NullableValue::Value(_) => {
                    let field_desc = desc.value_desc().get_field_desc(index);
                    Some(field_desc.get(value)?.to_vec())
                }
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

pub(crate) fn is_cross_partition_tx(tx: &dyn TxMgr) -> bool {
    if !tx.staged_put_items().is_empty() {
        return false;
    }
    let partitions = tx
        .staged_relation_ops()
        .keys()
        .map(|relation_id| relation_id.partition_id)
        .collect::<BTreeSet<_>>();
    partitions.len() > 1
}
