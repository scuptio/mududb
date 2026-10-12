#![allow(clippy::unwrap_used)]

use crate::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};

#[test]
fn table_partition_binding_fields() {
    let b = TablePartitionBinding {
        table_id: 1,
        rule_id: 2,
        ref_attr_indices: vec![0, 1],
    };
    assert_eq!(b.table_id, 1);
    assert_eq!(b.rule_id, 2);
    assert_eq!(b.ref_attr_indices, vec![0, 1]);
}

#[test]
fn partition_placement_fields() {
    let p = PartitionPlacement {
        partition_id: 3,
        worker_id: 4,
    };
    assert_eq!(p.partition_id, 3);
    assert_eq!(p.worker_id, 4);
}

#[test]
fn serde_roundtrip() {
    let b = TablePartitionBinding {
        table_id: 10,
        rule_id: 20,
        ref_attr_indices: vec![0],
    };
    let json = serde_json::to_string(&b).unwrap();
    let decoded: TablePartitionBinding = serde_json::from_str(&json).unwrap();
    assert_eq!(b, decoded);

    let p = PartitionPlacement {
        partition_id: 30,
        worker_id: 40,
    };
    let json = serde_json::to_string(&p).unwrap();
    let decoded: PartitionPlacement = serde_json::from_str(&json).unwrap();
    assert_eq!(p, decoded);
}
