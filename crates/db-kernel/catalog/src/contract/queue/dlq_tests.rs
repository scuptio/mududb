#![allow(clippy::unwrap_used)]
use super::*;
use crate::contract::{
    derive_oid::{derive_column_oid, derive_table_oid},
    fs_type::{FsColumnBinding, FsTypeKind},
};

#[test]
fn derived_dlq_identity_and_layout_are_stable_and_qualified() {
    let mut columns = queue_system_columns();
    let mut payload = SchemaColumn::new(
        "payload".into(),
        TypeFamily::U128,
        mudu_type::data_type_codec::from_opt_object(&DataType::default_for(TypeFamily::U128)),
    );
    payload.set_nullable(false);
    payload.set_fs_binding(Some(FsColumnBinding::new(9, FsTypeKind::File)));
    columns.push(payload);
    let values = (1..columns.len()).collect();
    let mut queue = SchemaClass::new("jobs".into(), columns, vec![0], values);
    queue.set_schema("tenant_a".into());
    queue.set_kind(SchemaClassKind::Queue);
    let first = queue_dlq_schema(&queue);
    let second = queue_dlq_schema(&queue);
    assert_eq!(
        rmp_serde::to_vec(&first).unwrap(),
        rmp_serde::to_vec(&second).unwrap()
    );
    assert_eq!(first.id(), derive_table_oid("tenant_a", "jobs_dlq"));
    for column in first.columns() {
        assert_eq!(
            column.get_oid(),
            derive_column_oid("tenant_a", "jobs_dlq", column.get_name())
        );
    }
    let payload = &first.columns()[QUEUE_PAYLOAD_ATTR_START];
    assert!(!payload.nullable());
    assert_eq!(
        payload.fs_binding(),
        Some(FsColumnBinding::new(9, FsTypeKind::File))
    );
    assert_eq!(payload.type_id(), TypeFamily::U128);
    queue.set_schema("tenant_b".into());
    let foreign = queue_dlq_schema(&queue);
    assert_ne!(first.id(), foreign.id());
    assert_ne!(first.columns()[0].get_oid(), foreign.columns()[0].get_oid());
}
