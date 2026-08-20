//! DML hooks that create and bind filesystem objects for FS-bound columns.
//!
//! A table column carrying an `FsColumnBinding` physically stores a system
//! generated object id (`U128`). These hooks run inside the key/value DML
//! executors and
//! - reject explicit user values for FS-bound columns,
//! - assign a fresh object id and write it into the row datum,
//! - produce the `_fs_object` catalog writes (a `PENDING` row per new object,
//!   a delete per replaced or removed object) that the executors stage into
//!   the transaction once the row operation itself succeeded.

use std::sync::Arc;

use mudu::common::buf::Buf;
use mudu::common::id::{AttrIndex, OID};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

use crate::contract::meta_mgr::MetaMgr;
use crate::contract::table_desc::TableDesc;
use crate::meta::fs_object::{
    decode_fs_oid_datum, encode_fs_object_key, encode_fs_object_row, encode_fs_oid_datum,
    gen_fs_oid, FsObjectRow, FS_OBJECT_STATE_PENDING, FS_OBJECT_TABLE_ID,
};
use crate::server::partition_router::{PartitionRouter, DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID};
use crate::x_engine::api::{OptRead, VecDatum, VecSelTerm, XContract};
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};

/// An FS-bound column of a table: its position in the row datum plus the
/// filesystem type it binds to.
pub(crate) struct FsBoundColumn {
    attr_index: AttrIndex,
    fs_id: u64,
    kind: u32,
}

/// A staged `_fs_object` write produced by the DML hooks.
pub(crate) enum FsStagedOp {
    /// Insert or replace the `_fs_object` row `key` with `value`.
    Put {
        partition_id: OID,
        key: Vec<u8>,
        value: Vec<u8>,
    },
    /// Delete the `_fs_object` row `key`.
    Delete { partition_id: OID, key: Vec<u8> },
}

/// Collect the FS-bound columns of `desc` in column definition order.
pub(crate) fn fs_bound_columns(desc: &TableDesc) -> Vec<FsBoundColumn> {
    desc.fields()
        .iter()
        .enumerate()
        .filter_map(|(index, field)| {
            field.fs_binding().map(|binding| FsBoundColumn {
                attr_index: index,
                fs_id: binding.fs_id(),
                kind: binding.kind().as_u32(),
            })
        })
        .collect()
}

/// Return true when `desc` has at least one FS-bound column.
pub(crate) fn has_fs_bound_columns(desc: &TableDesc) -> bool {
    desc.fields()
        .iter()
        .any(|field| field.fs_binding().is_some())
}

/// Return true when the update payload `value` assigns an FS-bound column.
pub(crate) fn update_touches_fs_columns(desc: &TableDesc, value: &VecDatum) -> bool {
    value.data().iter().any(|(attr, _)| {
        desc.fields()
            .get(*attr)
            .map(|field| field.fs_binding().is_some())
            .unwrap_or(false)
    })
}

/// Route `key` to its partition and reject FS-column DML that would stage
/// `_fs_object` rows on a partition owned by a different worker.
async fn resolve_fs_partition(
    meta_mgr: &Arc<dyn MetaMgr>,
    x_contract: &Arc<dyn XContract>,
    table_id: OID,
    desc: &TableDesc,
    key: &VecDatum,
) -> RS<OID> {
    let partition_id = PartitionRouter::new(meta_mgr.clone())
        .route_exact_partition(table_id, desc, key)
        .await?
        .unwrap_or(DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID);
    if let Some(worker_id) = meta_mgr.get_partition_worker(partition_id).await? {
        let local_worker_id = x_contract.local_worker_id();
        if local_worker_id != 0 && worker_id != local_worker_id {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                "fs column DML on a remote partition is not supported yet"
            ));
        }
    }
    Ok(partition_id)
}

/// Bind the FS-bound columns of an inserted row: assign a fresh object id to
/// every FS-bound column and return the `_fs_object` `PENDING` rows to stage
/// after the insert succeeds.
pub(crate) async fn bind_fs_columns_on_insert(
    meta_mgr: &Arc<dyn MetaMgr>,
    x_contract: &Arc<dyn XContract>,
    table_id: OID,
    desc: &TableDesc,
    key: &VecDatum,
    row: &mut VecDatum,
) -> RS<Vec<FsStagedOp>> {
    let columns = fs_bound_columns(desc);
    if columns.is_empty() {
        return Ok(Vec::new());
    }
    let partition_id = resolve_fs_partition(meta_mgr, x_contract, table_id, desc, key).await?;
    let mut ops = Vec::with_capacity(columns.len());
    for column in &columns {
        if find_datum(row, column.attr_index).is_some() {
            return Err(explicit_fs_value_error(desc, column));
        }
        let oid = gen_fs_oid();
        set_datum(row, column.attr_index, encode_fs_oid_datum(oid));
        ops.push(pending_object_put_op(column, oid, partition_id)?);
    }
    Ok(ops)
}

/// Rebind the FS-bound columns touched by an update: assign a fresh object id
/// per touched column, return its `_fs_object` `PENDING` row, and return a
/// delete for every object id the old row referenced.
///
/// The update payload marks a touched FS-bound column with an empty datum;
/// any non-empty (explicit) value is rejected.
pub(crate) async fn rebind_fs_columns_on_update(
    meta_mgr: &Arc<dyn MetaMgr>,
    x_contract: &Arc<dyn XContract>,
    tx_mgr: &Arc<dyn TxMgr>,
    table_id: OID,
    desc: &TableDesc,
    key: &VecDatum,
    value: &mut VecDatum,
) -> RS<Vec<FsStagedOp>> {
    let columns = fs_bound_columns(desc)
        .into_iter()
        .filter(|column| find_datum(value, column.attr_index).is_some())
        .collect::<Vec<_>>();
    if columns.is_empty() {
        return Ok(Vec::new());
    }
    let partition_id = resolve_fs_partition(meta_mgr, x_contract, table_id, desc, key).await?;
    let select = VecSelTerm::new(columns.iter().map(|column| column.attr_index).collect());
    let old_row = x_contract
        .read_key(tx_mgr.clone(), table_id, key, &select, &OptRead::default())
        .await?;
    let mut ops = Vec::with_capacity(columns.len() * 2);
    for (position, column) in columns.iter().enumerate() {
        let provided = find_datum(value, column.attr_index);
        if let Some(datum) = provided {
            if !datum.is_empty() {
                return Err(explicit_fs_value_error(desc, column));
            }
        }
        let oid = gen_fs_oid();
        set_datum(value, column.attr_index, encode_fs_oid_datum(oid));
        ops.push(pending_object_put_op(column, oid, partition_id)?);
        if let Some(old_fields) = &old_row {
            if let Some(Some(old_datum)) = old_fields.get(position) {
                ops.push(FsStagedOp::Delete {
                    partition_id,
                    key: encode_fs_object_key(decode_fs_oid_datum(old_datum)?)?,
                });
            }
        }
    }
    Ok(ops)
}

/// Unbind the FS-bound columns of a deleted row: return a `_fs_object` delete
/// for every object id the row referenced.
pub(crate) async fn unbind_fs_columns_on_delete(
    meta_mgr: &Arc<dyn MetaMgr>,
    x_contract: &Arc<dyn XContract>,
    tx_mgr: &Arc<dyn TxMgr>,
    table_id: OID,
    desc: &TableDesc,
    key: &VecDatum,
) -> RS<Vec<FsStagedOp>> {
    let columns = fs_bound_columns(desc);
    if columns.is_empty() {
        return Ok(Vec::new());
    }
    let partition_id = resolve_fs_partition(meta_mgr, x_contract, table_id, desc, key).await?;
    let select = VecSelTerm::new(columns.iter().map(|column| column.attr_index).collect());
    let old_row = x_contract
        .read_key(tx_mgr.clone(), table_id, key, &select, &OptRead::default())
        .await?;
    let Some(old_fields) = old_row else {
        return Ok(Vec::new());
    };
    let mut ops = Vec::with_capacity(columns.len());
    for old_datum in old_fields.iter().flatten() {
        ops.push(FsStagedOp::Delete {
            partition_id,
            key: encode_fs_object_key(decode_fs_oid_datum(old_datum)?)?,
        });
    }
    Ok(ops)
}

/// Apply the staged `_fs_object` operations to the transaction.
pub(crate) fn stage_fs_ops(tx_mgr: &Arc<dyn TxMgr>, ops: Vec<FsStagedOp>) {
    for op in ops {
        match op {
            FsStagedOp::Put {
                partition_id,
                key,
                value,
            } => tx_mgr.put_relation(
                PhysicalRelationId {
                    table_id: FS_OBJECT_TABLE_ID,
                    partition_id,
                },
                key,
                value,
            ),
            FsStagedOp::Delete { partition_id, key } => tx_mgr.delete_relation(
                PhysicalRelationId {
                    table_id: FS_OBJECT_TABLE_ID,
                    partition_id,
                },
                key,
            ),
        }
    }
}

fn pending_object_put_op(column: &FsBoundColumn, oid: OID, partition_id: OID) -> RS<FsStagedOp> {
    Ok(FsStagedOp::Put {
        partition_id,
        key: encode_fs_object_key(oid)?,
        value: encode_fs_object_row(&FsObjectRow {
            fs_id: column.fs_id,
            kind: column.kind,
            generation: 0,
            length: 0,
            state: FS_OBJECT_STATE_PENDING,
        })?,
    })
}

fn explicit_fs_value_error(desc: &TableDesc, column: &FsBoundColumn) -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::InvalidArgument,
        format!(
            "fs column {} values are assigned by the system",
            desc.get_attr(column.attr_index).name()
        )
    )
}

fn find_datum(row: &VecDatum, attr_index: AttrIndex) -> Option<&Buf> {
    row.data()
        .iter()
        .find(|(attr, _)| *attr == attr_index)
        .map(|(_, datum)| datum)
}

fn set_datum(row: &mut VecDatum, attr_index: AttrIndex, datum: Buf) {
    let mut data = std::mem::take(row).into_data();
    match data.iter_mut().find(|(attr, _)| *attr == attr_index) {
        Some((_, slot)) => *slot = datum,
        None => data.push((attr_index, datum)),
    }
    *row = VecDatum::new(data);
}
