//! Idobject checks shared by local DDL and committed control replay.
use super::queue_dlq_name;
use crate::contract::{derive_oid::derive_table_oid, meta_mgr::MetaMgr, table_desc::TableDesc};
use mudu::{common::result::RS, error::ErrorCode, mudu_error};
use std::sync::Arc;

/// Resolves only the queue's derived DLQ in its exact namespace.
/// Missing DLQs are allowed for interrupted drops; conflicting identities or
/// object kinds fail before either object is removed. Callers must serialize
/// catalog mutation across this check and the subsequent drops.
pub async fn resolve_queue_dlq_for_drop(
    meta: &dyn MetaMgr,
    queue: &TableDesc,
) -> RS<Option<Arc<TableDesc>>> {
    if !queue.is_queue() {
        return Err(mudu_error!(
            ErrorCode::ObjectNotFound,
            "drop target is not a queue"
        ));
    }
    let name = queue_dlq_name(queue.name());
    let dlq = meta.get_table_exact(queue.schema(), &name).await?;
    if let Some(desc) = &dlq {
        if !desc.is_queue()
            || desc.schema() != queue.schema()
            || desc.name() != &name
            || desc.id() != derive_table_oid(queue.schema(), &name)
        {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "dead-letter queue identity conflicts with its owner"
            ));
        }
    }
    Ok(dlq)
}
