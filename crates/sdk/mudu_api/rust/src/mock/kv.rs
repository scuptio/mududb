//! In-memory debug emulation of the session, KV and relation syscall
//! families (message kinds `Open`..`Range` and `RelationGet`..
//! `RelationInsert`). Sessions are allocated from a monotonically increasing
//! counter; each session oid owns an independent ordered key/value store.
//! Relations are per-`(session, table)` row lists addressed by exact primary
//! key equality. All state is static, process-wide debug state: nothing is
//! persisted.

use crate::mudu_sys::relation::{RelationColumn, RelationRow};
use crate::types::UniReturn;
use crate::universal::uni_error::UniError;
use crate::universal::uni_oid::UniOid;
use crate::universal::uni_relation::{
    RELATION_DELTA_OP_ADD, RELATION_DELTA_OP_ADD_DEFERRED, RELATION_DELTA_OP_SUB,
    RELATION_DELTA_OP_SUB_DEFERRED, RELATION_DELTA_OP_SUB_WRAP_DEFERRED, UniRelationDelta,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

type ObjectKey = (u64, u64);

/// Per-session ordered key/value stores.
type KvStores = HashMap<ObjectKey, BTreeMap<Vec<u8>, Vec<u8>>>;

/// Per-`(session, table)` relation row lists.
type RelationStores = HashMap<(ObjectKey, String), Vec<StoredRow>>;

/// A stored relation row: primary-key columns plus value columns.
struct StoredRow {
    key: Vec<RelationColumn>,
    values: Vec<RelationColumn>,
}

fn next_session() -> &'static AtomicU64 {
    static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);
    &NEXT_SESSION
}

fn sessions() -> &'static RwLock<HashSet<ObjectKey>> {
    static SESSIONS: OnceLock<RwLock<HashSet<ObjectKey>>> = OnceLock::new();
    SESSIONS.get_or_init(|| RwLock::new(HashSet::new()))
}

fn stores() -> &'static RwLock<KvStores> {
    static STORES: OnceLock<RwLock<KvStores>> = OnceLock::new();
    STORES.get_or_init(|| RwLock::new(HashMap::new()))
}

fn relations() -> &'static RwLock<RelationStores> {
    static RELATIONS: OnceLock<RwLock<RelationStores>> = OnceLock::new();
    RELATIONS.get_or_init(|| RwLock::new(HashMap::new()))
}

fn oid_key(oid: &UniOid) -> ObjectKey {
    (oid.h, oid.l)
}

fn error<T>(message: String) -> UniReturn<T> {
    Err(UniError {
        err_code: 1,
        err_msg: message,
        err_src: "MockKvEmulation".to_string(),
        ..Default::default()
    })
}

/// Returns the session key, or an error when the session is not open.
fn require_session(oid: &UniOid) -> UniReturn<ObjectKey> {
    let key = oid_key(oid);
    if !sessions()
        .read()
        .expect("mock sessions lock poisoned")
        .contains(&key)
    {
        return error(format!("unknown session oid ({},{})", oid.h, oid.l));
    }
    Ok(key)
}

pub(super) fn open(_worker_id: UniOid) -> UniReturn<UniOid> {
    let id = next_session().fetch_add(1, Ordering::Relaxed);
    let oid = UniOid { h: 0, l: id };
    sessions()
        .write()
        .expect("mock sessions lock poisoned")
        .insert(oid_key(&oid));
    Ok(oid)
}

pub(super) fn close(oid: UniOid) -> UniReturn<()> {
    let key = require_session(&oid)?;
    sessions()
        .write()
        .expect("mock sessions lock poisoned")
        .remove(&key);
    stores()
        .write()
        .expect("mock kv stores lock poisoned")
        .remove(&key);
    Ok(())
}

pub(super) fn get(oid: UniOid, key: Vec<u8>) -> UniReturn<Option<Vec<u8>>> {
    let session = require_session(&oid)?;
    let stores = stores().read().expect("mock kv stores lock poisoned");
    Ok(stores
        .get(&session)
        .and_then(|store| store.get(&key))
        .cloned())
}

pub(super) fn put(oid: UniOid, key: Vec<u8>, value: Vec<u8>) -> UniReturn<()> {
    let session = require_session(&oid)?;
    stores()
        .write()
        .expect("mock kv stores lock poisoned")
        .entry(session)
        .or_default()
        .insert(key, value);
    Ok(())
}

pub(super) fn delete(oid: UniOid, key: Vec<u8>) -> UniReturn<()> {
    let session = require_session(&oid)?;
    if let Some(store) = stores()
        .write()
        .expect("mock kv stores lock poisoned")
        .get_mut(&session)
    {
        store.remove(&key);
    }
    Ok(())
}

pub(super) fn range(
    oid: UniOid,
    start: Vec<u8>,
    end: Vec<u8>,
) -> UniReturn<Vec<(Vec<u8>, Vec<u8>)>> {
    let session = require_session(&oid)?;
    let stores = stores().read().expect("mock kv stores lock poisoned");
    let items = stores
        .get(&session)
        .map(|store| {
            store
                .range(start..end)
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default();
    Ok(items)
}

fn table_relations<'a>(
    relations: &'a HashMap<(ObjectKey, String), Vec<StoredRow>>,
    oid: &UniOid,
    table: &str,
) -> Option<&'a Vec<StoredRow>> {
    relations.get(&(oid_key(oid), table.to_string()))
}

fn find_row(rows: &[StoredRow], key: &[RelationColumn]) -> Option<usize> {
    rows.iter().position(|row| row.key == key)
}

fn column_datum(row: &StoredRow, attr: u64) -> Option<&[u8]> {
    row.key
        .iter()
        .chain(row.values.iter())
        .find(|(column_attr, _)| *column_attr == attr)
        .map(|(_, datum)| datum.as_slice())
}

fn set_column(columns: &mut Vec<RelationColumn>, attr: u64, datum: Vec<u8>) {
    match columns
        .iter_mut()
        .find(|(column_attr, _)| *column_attr == attr)
    {
        Some((_, existing)) => *existing = datum,
        None => columns.push((attr, datum)),
    }
}

pub(super) fn relation_insert(
    oid: UniOid,
    table: String,
    key: Vec<RelationColumn>,
    values: Vec<RelationColumn>,
) -> UniReturn<()> {
    require_session(&oid)?;
    let mut relations = relations().write().expect("mock relations lock poisoned");
    let rows = relations.entry((oid_key(&oid), table)).or_default();
    if find_row(rows, &key).is_some() {
        return error("relation-insert: duplicate key".to_string());
    }
    rows.push(StoredRow { key, values });
    Ok(())
}

pub(super) fn relation_get(
    oid: UniOid,
    table: String,
    key: Vec<RelationColumn>,
    select: Vec<u64>,
) -> UniReturn<RelationRow> {
    require_session(&oid)?;
    let relations = relations().read().expect("mock relations lock poisoned");
    let row = table_relations(&relations, &oid, &table)
        .and_then(|rows| find_row(rows, &key).map(|index| &rows[index]));
    Ok(row.map(|row| {
        select
            .iter()
            .map(|attr| column_datum(row, *attr).map(<[u8]>::to_vec))
            .collect()
    }))
}

/// Applies one delta to `current`, returning the new datum.
fn apply_delta(current: Option<&[u8]>, delta: &UniRelationDelta) -> UniReturn<Vec<u8>> {
    let current = current
        .map(|datum| {
            <[u8; 8]>::try_from(datum)
                .map(i64::from_be_bytes)
                .map_err(|_| "relation-update: current datum is not an 8-byte integer".to_string())
        })
        .transpose()
        .map_err(|message| UniError {
            err_code: 1,
            err_msg: message,
            err_src: "MockKvEmulation".to_string(),
            ..Default::default()
        })?
        .unwrap_or(0);

    let operand = |datum: &[u8]| -> UniReturn<i64> {
        <[u8; 8]>::try_from(datum)
            .map(i64::from_be_bytes)
            .map_err(|_| UniError {
                err_code: 1,
                err_msg: "relation-update: delta datum is not an 8-byte integer".to_string(),
                err_src: "MockKvEmulation".to_string(),
                ..Default::default()
            })
    };

    let updated = match delta.op {
        RELATION_DELTA_OP_ADD | RELATION_DELTA_OP_ADD_DEFERRED => current + operand(&delta.datum)?,
        RELATION_DELTA_OP_SUB | RELATION_DELTA_OP_SUB_DEFERRED => current - operand(&delta.datum)?,
        RELATION_DELTA_OP_SUB_WRAP_DEFERRED => {
            if delta.datum.len() != 24 {
                return error("relation-update: wrap delta datum is not 24 bytes".to_string());
            }
            let quantity = operand(&delta.datum[0..8])?;
            let floor = operand(&delta.datum[8..16])?;
            let wrap = operand(&delta.datum[16..24])?;
            let mut updated = current - quantity;
            if updated < floor {
                updated += wrap;
            }
            updated
        }
        other => return error(format!("relation-update: unsupported delta op {other}")),
    };
    Ok(updated.to_be_bytes().to_vec())
}

pub(super) fn relation_update(
    oid: UniOid,
    table: String,
    key: Vec<RelationColumn>,
    values: Vec<RelationColumn>,
    deltas: Vec<UniRelationDelta>,
) -> UniReturn<u64> {
    require_session(&oid)?;
    let mut relations = relations().write().expect("mock relations lock poisoned");
    let rows = relations.entry((oid_key(&oid), table)).or_default();
    let Some(index) = find_row(rows, &key) else {
        return Ok(0);
    };
    let row = &mut rows[index];
    for (attr, datum) in values {
        set_column(&mut row.values, attr, datum);
    }
    for delta in &deltas {
        let updated = apply_delta(column_datum(row, delta.attr), delta)?;
        set_column(&mut row.values, delta.attr, updated);
    }
    Ok(1)
}
