#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;

fn rule_with_bounds(name: &str) -> PartitionRuleDesc {
    PartitionRuleDesc::new_range(
        name.to_string(),
        vec![TypeFamily::I32],
        vec![
            RangePartitionDef::new(
                "p0".to_string(),
                PartitionBound::Unbounded,
                PartitionBound::Value(vec![b"100".to_vec()]),
            ),
            RangePartitionDef::new(
                "p1".to_string(),
                PartitionBound::Value(vec![b"100".to_vec()]),
                PartitionBound::Unbounded,
            ),
        ],
    )
}

fn partitioned_binder() -> Binder {
    let schema = SchemaTable::new(
        "orders".to_string(),
        vec![
            SchemaColumn::new(
                "region_id".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
            SchemaColumn::new(
                "order_id".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
            SchemaColumn::new(
                "amount".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
        ],
        vec![0, 1],
        vec![2],
    );
    binder_arc(Arc::new(TestMetaMgr::with_rule(
        schema,
        rule_with_bounds("r_orders"),
    )))
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_partition_rule_infers_key_types_from_bounds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(
                parse_stmt(
                    "CREATE PARTITION RULE r_test RANGE (
                        PARTITION p0 VALUES FROM (MINVALUE) TO (100),
                        PARTITION p1 VALUES FROM (100) TO (MAXVALUE)
                    );",
                ),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::CreatePartitionRule(rule)) = bound else {
            panic!("expected create partition rule");
        };
        assert_eq!(rule.rule.key_types, vec![TypeFamily::I64]);
        assert_eq!(rule.rule.partitions.len(), 2);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_partition_placement_resolves_partition_and_worker() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = partitioned_binder()
            .bind(
                parse_stmt(
                    "CREATE PARTITION PLACEMENT FOR RULE r_orders (
                        PARTITION p0 ON WORKER 11,
                        PARTITION p1 ON WORKER 12
                    );",
                ),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::CreatePartitionPlacement(placement)) = bound else {
            panic!("expected create partition placement");
        };
        assert_eq!(placement.placements.len(), 2);
        assert_eq!(placement.placements[0].worker_id, 11);
        assert_eq!(placement.placements[1].worker_id, 12);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_table_with_partition_binding_resolves_rule() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = partitioned_binder()
            .bind(
                parse_stmt(
                    "CREATE TABLE orders (
                        region_id INT,
                        order_id INT,
                        amount INT,
                        PRIMARY KEY (region_id, order_id)
                    ) PARTITION BY GLOBAL RULE r_orders REFERENCES (region_id, order_id);",
                ),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::CreateTable(create)) = bound else {
            panic!("expected create table");
        };
        assert!(create.partition_binding.is_some());
        assert_eq!(
            create.partition_binding.as_ref().unwrap().ref_attr_indices,
            vec![0, 1]
        );
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_table_fails_when_partition_rule_not_found() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(
                parse_stmt(
                    "CREATE TABLE missing (
                        id INT PRIMARY KEY
                    ) PARTITION BY GLOBAL RULE no_such_rule REFERENCES (id);",
                ),
                &(),
            )
            .await
            .unwrap_err();

        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_table_fails_when_partition_reference_column_not_found() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = partitioned_binder()
            .bind(
                parse_stmt(
                    "CREATE TABLE orders (
                        region_id INT,
                        order_id INT,
                        amount INT,
                        PRIMARY KEY (region_id, order_id)
                    ) PARTITION BY GLOBAL RULE r_orders REFERENCES (region_id, missing_col);",
                ),
                &(),
            )
            .await
            .unwrap_err();

        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_partition_rule_rejects_unbounded_only_rule() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(
                parse_stmt(
                    "CREATE PARTITION RULE r_empty RANGE (
                        PARTITION p0 VALUES FROM (MINVALUE) TO (MAXVALUE)
                    );",
                ),
                &(),
            )
            .await
            .unwrap_err();

        assert!(err.to_string().contains("cannot infer partition key types"));
    })
    .unwrap()
}
