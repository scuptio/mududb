//! Session, KV and relation frame handlers: decode the request frame, run
//! the in-memory emulation in [`crate::mock::kv`] and encode the response.

use super::MockSqliteMuduSysCall;
use crate::error::ApiError;
use crate::mudu_sys::{kv as sys_kv, relation, session};

impl MockSqliteMuduSysCall {
    /// Routes an `open-session` request frame to the in-memory emulation.
    pub(super) fn route_open(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let worker_id = session::decode_open_request(frame)?;
        session::encode_open_response(&crate::mock::kv::open(worker_id))
    }

    /// Routes a `close-session` request frame to the in-memory emulation.
    pub(super) fn route_close(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let oid = session::decode_close_request(frame)?;
        session::encode_close_response(&crate::mock::kv::close(oid))
    }

    /// Routes a `get` request frame to the in-memory emulation.
    pub(super) fn route_get(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (oid, key) = sys_kv::decode_get_request(frame)?;
        sys_kv::encode_get_response(&crate::mock::kv::get(oid, key))
    }

    /// Routes a `put` request frame to the in-memory emulation.
    pub(super) fn route_put(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (oid, key, value) = sys_kv::decode_put_request(frame)?;
        sys_kv::encode_put_response(&crate::mock::kv::put(oid, key, value))
    }

    /// Routes a `delete` request frame to the in-memory emulation.
    pub(super) fn route_delete(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (oid, key) = sys_kv::decode_delete_request(frame)?;
        sys_kv::encode_delete_response(&crate::mock::kv::delete(oid, key))
    }

    /// Routes a `range` request frame to the in-memory emulation.
    pub(super) fn route_range(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (oid, start, end) = sys_kv::decode_range_request(frame)?;
        sys_kv::encode_range_response(&crate::mock::kv::range(oid, start, end))
    }

    /// Routes a `relation-get` request frame to the in-memory emulation.
    pub(super) fn route_relation_get(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (oid, table, key, select) = relation::decode_relation_get_request(frame)?;
        relation::encode_relation_get_response(&crate::mock::kv::relation_get(
            oid, table, key, select,
        ))
    }

    /// Routes a `relation-update` request frame to the in-memory emulation.
    pub(super) fn route_relation_update(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (oid, table, key, values, deltas) = relation::decode_relation_update_request(frame)?;
        relation::encode_relation_update_response(&crate::mock::kv::relation_update(
            oid, table, key, values, deltas,
        ))
    }

    /// Routes a `relation-insert` request frame to the in-memory emulation.
    pub(super) fn route_relation_insert(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (oid, table, key, values) = relation::decode_relation_insert_request(frame)?;
        relation::encode_relation_insert_response(&crate::mock::kv::relation_insert(
            oid, table, key, values,
        ))
    }
}
