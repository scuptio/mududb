//! Resumable substeps of catalog creation after interrupted control apply.
use super::*;
use catalog::contract::{
    meta_mgr::MetaMgr, partition_rule_binding::TablePartitionBinding, schema_class::SchemaClass,
};

pub(super) async fn ensure_object(meta: &dyn MetaMgr, schema: &SchemaClass, queue: bool) -> RS<()> {
    if !verify_existing_object(meta, schema, queue).await? {
        if queue {
            meta.create_queue(schema).await?;
        } else {
            meta.create_table(schema).await?;
        }
    }
    Ok(())
}

pub(super) async fn verify_existing_object(
    meta: &dyn MetaMgr,
    schema: &SchemaClass,
    queue: bool,
) -> RS<bool> {
    if schema.is_queue() != queue
        || (queue && schema.columns().len() < catalog::contract::queue::QUEUE_PAYLOAD_ATTR_START)
    {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "catalog operation has incompatible object kind or queue layout"
        ));
    }
    match meta.get_table_by_id(schema.id()).await {
        Ok(existing) => {
            if existing.name() != schema.class_name()
                || existing.schema() != schema.schema()
                || existing.is_queue() != schema.is_queue()
            {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    "replayed catalog object identity conflict"
                ));
            }
            let persisted = meta
                .list_schemas()
                .await?
                .into_iter()
                .find(|candidate| candidate.id() == schema.id())
                .ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::InvalidState,
                        "catalog object has no recoverable schema"
                    )
                })?;
            ensure_same_schema(&persisted, schema)?;
            Ok(true)
        }
        Err(error) if error.ec() == ErrorCode::ObjectNotFound => {
            if meta
                .get_table_exact(schema.schema(), schema.class_name())
                .await?
                .is_some()
            {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    "catalog name belongs to a different object identity"
                ));
            }
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

/// Validate the complete binding intent before any object creation substep.
/// The caller serializes metadata apply across this check and the writes.
pub(super) async fn verify_binding(
    meta: &dyn MetaMgr,
    schema: &SchemaClass,
    binding: Option<&TablePartitionBinding>,
) -> RS<()> {
    if let Some(binding) = binding {
        if binding.table_id != schema.id() {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "partition binding belongs to a different catalog object"
            ));
        }
        // A missing rule must not leave a newly created table/queue behind.
        let rule = meta.get_partition_rule_by_id(binding.rule_id).await?;
        binding.validate(schema, &rule)?;
    }
    let existing = meta.get_table_partition_binding(schema.id()).await?;
    if existing
        .as_ref()
        .is_some_and(|existing| Some(existing) != binding)
    {
        return Err(mudu_error!(
            ErrorCode::InvalidState,
            "catalog binding conflicts with the complete create intent"
        ));
    }
    Ok(())
}

pub(super) async fn ensure_binding(
    meta: &dyn MetaMgr,
    binding: Option<&TablePartitionBinding>,
) -> RS<()> {
    let Some(binding) = binding else {
        return Ok(());
    };
    match meta.get_table_partition_binding(binding.table_id).await? {
        Some(existing) if existing == *binding => Ok(()),
        Some(_) => Err(mudu_error!(
            ErrorCode::InvalidState,
            "replayed catalog partition binding conflict"
        )),
        None => meta.bind_table_partition(binding).await,
    }
}

/// Compare the complete wire schema, including field identities, types,
/// nullability, key/value layout and filesystem bindings.
pub(super) fn ensure_same_schema(existing: &SchemaClass, expected: &SchemaClass) -> RS<()> {
    let encode = |schema: &SchemaClass| {
        rmp_serde::to_vec(schema)
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode catalog schema", e))
    };
    if encode(existing)? != encode(expected)? {
        return Err(mudu_error!(
            ErrorCode::InvalidState,
            "replayed catalog schema conflicts with durable definition"
        ));
    }
    Ok(())
}

pub(super) async fn cleanup_orphan_binding(
    meta: &dyn MetaMgr,
    oid: mudu::common::id::OID,
) -> RS<()> {
    if meta.get_table_partition_binding(oid).await?.is_some() {
        meta.remove_orphan_partition_binding(oid).await?;
    }
    Ok(())
}

pub(super) async fn drop_queue(meta_mgr: &dyn MetaMgr, oid: mudu::common::id::OID) -> RS<()> {
    match meta_mgr.get_table_by_id(oid).await {
        Ok(desc) => {
            // The derived dead-letter queue goes with the queue.
            if let Some(dlq) =
                ::catalog::contract::queue::resolve_queue_dlq_for_drop(meta_mgr, desc.as_ref())
                    .await?
            {
                meta_mgr.drop_queue(dlq.id()).await?;
            } else {
                let dlq_id = catalog::contract::derive_oid::derive_table_oid(
                    desc.schema(),
                    &catalog::contract::queue::queue_dlq_name(desc.name()),
                );
                cleanup_orphan_binding(meta_mgr, dlq_id).await?;
            }
            meta_mgr.drop_queue(oid).await
        }
        Err(error) if error.ec() == ErrorCode::ObjectNotFound => {
            cleanup_orphan_binding(meta_mgr, oid).await
        }
        Err(error) => Err(error),
    }
}
