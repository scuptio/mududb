#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;
use ::catalog::contract::partition_rule::{PartitionBound, PartitionRuleDesc, RangePartitionDef};
use ::catalog::contract::queue::{
    queue_system_columns, QUEUE_LSN_COLUMN, QUEUE_PAYLOAD_ATTR_START, QUEUE_RECEIVE_COUNT_COLUMN,
    QUEUE_SEQ_COLUMN, QUEUE_STATUS_COLUMN,
};
use ::catalog::contract::schema_table::SchemaObjectKind;

fn queue_i32_rule(name: &str) -> PartitionRuleDesc {
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

fn queue_schema(name: &str) -> SchemaTable {
    let columns = queue_system_columns();
    let value_indices = (1..columns.len()).collect();
    let mut schema = SchemaTable::new(name.to_string(), columns, vec![0], value_indices);
    schema.set_kind(SchemaObjectKind::Queue);
    schema
}

fn queue_schema_with_payload(name: &str) -> SchemaTable {
    let mut columns = queue_system_columns();
    columns.push(SchemaColumn::new(
        "body".to_string(),
        TypeFamily::String,
        DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::String)),
    ));
    let value_indices = (1..columns.len()).collect();
    let mut schema = SchemaTable::new(name.to_string(), columns, vec![0], value_indices);
    schema.set_kind(SchemaObjectKind::Queue);
    schema
}

fn assert_system_column_layout(schema: &SchemaTable, payload_names: &[&str]) {
    assert_eq!(schema.kind(), SchemaObjectKind::Queue);
    assert!(schema.is_queue());
    assert_eq!(schema.key_indices(), &vec![0]);
    assert_eq!(
        schema.value_indices(),
        &(1..schema.columns().len()).collect::<Vec<_>>()
    );
    let system = [
        (QUEUE_SEQ_COLUMN, TypeFamily::I64, false),
        (QUEUE_LSN_COLUMN, TypeFamily::I64, false),
        (QUEUE_STATUS_COLUMN, TypeFamily::I32, false),
        (QUEUE_RECEIVE_COUNT_COLUMN, TypeFamily::I32, false),
        ("__state_hi", TypeFamily::U128, true),
        ("__state_lo", TypeFamily::U128, true),
    ];
    let columns = schema.columns();
    assert_eq!(columns.len(), system.len() + payload_names.len());
    for (column, (name, family, nullable)) in columns.iter().zip(system.iter()) {
        assert_eq!(column.get_name(), name);
        assert_eq!(column.type_id(), *family);
        assert_eq!(column.nullable(), *nullable);
    }
    assert!(columns[0].is_primary(), "__seq is the queue primary key");
    for (index, column) in columns[1..].iter().enumerate() {
        assert!(
            !column.is_primary(),
            "value column {} must not be a key column",
            column.get_name()
        );
        assert_eq!(column.get_index(), index, "value-tuple index is normalized");
    }
    for (column, name) in columns[system.len()..].iter().zip(payload_names.iter()) {
        assert_eq!(column.get_name(), name);
    }
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_queue_with_columns_builds_system_layout() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(
                parse_stmt("create queue jobs (body varchar(100) not null, retries int);"),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::CreateQueue(create)) = bound else {
            panic!("expected create queue");
        };
        let schema = &create.schema;
        assert_eq!(schema.table_name(), "jobs");
        assert_eq!(schema.schema(), &DEFAULT_SCHEMA.to_string());
        assert_system_column_layout(schema, &["body", "retries"]);
        let body = &schema.columns()[6];
        assert_eq!(body.type_id(), TypeFamily::String);
        assert!(
            !body.nullable(),
            "NOT NULL survives into the payload column"
        );
        let retries = &schema.columns()[7];
        assert_eq!(retries.type_id(), TypeFamily::I32);
        assert!(retries.nullable());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_queue_like_copies_columns_without_key_marks() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        // users(id INT PRIMARY KEY, name VARCHAR): id is a non-nullable key
        // column, name is a nullable value column in the source table.
        let bound = binder()
            .bind(parse_stmt("create queue jobs like users;"), &())
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::CreateQueue(create)) = bound else {
            panic!("expected create queue");
        };
        let schema = &create.schema;
        assert_system_column_layout(schema, &["id", "name"]);
        let id = &schema.columns()[6];
        assert_eq!(id.type_id(), TypeFamily::I32);
        assert!(
            !id.nullable(),
            "nullability is inherited from the source column"
        );
        let name = &schema.columns()[7];
        assert_eq!(name.type_id(), TypeFamily::String);
        assert!(name.nullable());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_queue_rejects_user_primary_key() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(parse_stmt("create queue jobs (a int primary key);"), &())
            .await
            .unwrap_err();

        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_queue_rejects_reserved_payload_name() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(parse_stmt("create queue jobs (__seq bigint);"), &())
            .await
            .unwrap_err();

        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_queue_like_rejects_queue_source() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        // LIKE over another queue would copy its system columns into the
        // payload, so the reserved-name guard rejects it.
        let err = binder_arc(Arc::new(TestMetaMgr::new(queue_schema("src_q"))))
            .bind(parse_stmt("create queue jobs like src_q;"), &())
            .await
            .unwrap_err();

        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_queue_like_unknown_table_fails() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(parse_stmt("create queue jobs like missing;"), &())
            .await
            .unwrap_err();

        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_drop_queue_returns_oid_for_queue() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let schema = queue_schema("jobs");
        let queue_oid = schema.id();
        let bound = binder_arc(Arc::new(TestMetaMgr::new(schema)))
            .bind(parse_stmt("drop queue jobs;"), &())
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::DropQueue(drop)) = bound else {
            panic!("expected drop queue");
        };
        assert_eq!(drop.oid, queue_oid);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_drop_queue_fails_on_plain_table() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(parse_stmt("drop queue users;"), &())
            .await
            .unwrap_err();

        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_drop_queue_fails_when_missing() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(parse_stmt("drop queue missing;"), &())
            .await
            .unwrap_err();

        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_receive_resolves_queue_and_builds_envelope() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let schema = queue_schema_with_payload("jobs");
        let queue_oid = schema.id();
        let bound = binder_arc(Arc::new(TestMetaMgr::new(schema)))
            .bind(parse_stmt("receive from jobs;"), &())
            .await
            .unwrap();

        let BoundStmt::Query(BoundQuery::Receive(receive)) = bound else {
            panic!("expected receive query");
        };
        assert_eq!(receive.table_id, queue_oid);
        assert_eq!(receive.count, 1, "RECEIVE without a count claims one");
        let names: Vec<&str> = receive
            .tuple_desc
            .fields()
            .iter()
            .map(|field| field.name())
            .collect();
        assert_eq!(names, vec!["__seq", "__lsn", "body"]);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_receive_with_count_and_wait() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let schema = queue_schema_with_payload("jobs");
        let bound = binder_arc(Arc::new(TestMetaMgr::new(schema)))
            .bind(parse_stmt("receive 3 from jobs wait 500;"), &())
            .await
            .unwrap();

        let BoundStmt::Query(BoundQuery::Receive(receive)) = bound else {
            panic!("expected receive query");
        };
        assert_eq!(receive.count, 3);
        assert!(receive.wait.is_some());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_receive_fails_on_missing_queue_and_plain_table() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(parse_stmt("receive from missing;"), &())
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);

        // `users` is a plain table, not a queue.
        let err = binder()
            .bind(parse_stmt("receive from users;"), &())
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_ack_resolves_queue() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let schema = queue_schema_with_payload("jobs");
        let queue_oid = schema.id();
        let bound = binder_arc(Arc::new(TestMetaMgr::new(schema)))
            .bind(parse_stmt("ack jobs to 41;"), &())
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Ack(ack)) = bound else {
            panic!("expected ack command");
        };
        assert_eq!(ack.table_id, queue_oid);
        assert_eq!(ack.seq, 41);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_insert_into_queue_maps_payload_columns_only() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder_arc(Arc::new(TestMetaMgr::new(queue_schema_with_payload(
            "jobs",
        ))))
        .bind(parse_stmt("insert into jobs values ('hello');"), &())
        .await
        .unwrap();

        let BoundStmt::Command(BoundCommand::Insert(insert)) = bound else {
            panic!("expected bound insert");
        };
        assert_eq!(insert.rows.len(), 1);
        assert!(
            insert.rows[0].key.is_empty(),
            "the __seq key is synthesized by the executor"
        );
        assert_eq!(insert.rows[0].value.len(), 1);
        assert_eq!(
            insert.rows[0].value[0].0, 6,
            "the payload column keeps its attribute index past the system columns"
        );
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_insert_into_queue_rejects_system_columns() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        for sql in [
            "insert into jobs (__seq, body) values (1, 'x');",
            "insert into jobs (status, body) values (0, 'x');",
        ] {
            let err = binder_arc(Arc::new(TestMetaMgr::new(queue_schema_with_payload(
                "jobs",
            ))))
            .bind(parse_stmt(sql), &())
            .await
            .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::InvalidArgument, "sql: {sql}");
        }
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn insert_template_for_queue_is_classified_other() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let binder = binder_arc(Arc::new(TestMetaMgr::new(queue_schema_with_payload(
            "jobs",
        ))));
        let stmt = parse_stmt("insert into jobs values (?);");
        let template = binder.bind_template(&stmt).await.unwrap().unwrap();
        let crate::bound_template::StmtTemplate::Insert(insert) = &template.stmt else {
            panic!("expected insert template");
        };
        assert!(insert.is_queue);
        assert!(matches!(
            template.classify(),
            crate::bound_template::PlanClass::Other
        ));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn update_and_delete_on_queue_are_rejected() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let binder = binder_arc(Arc::new(TestMetaMgr::new(queue_schema_with_payload(
            "jobs",
        ))));
        // Queue rows are managed exclusively through INSERT (enqueue),
        // RECEIVE (claim) and ACK (delete); raw UPDATE/DELETE would corrupt
        // the claim state.
        let err = binder
            .bind(
                parse_stmt("update jobs set body = 'x' where __seq = 1;"),
                &(),
            )
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
        let err = binder
            .bind(parse_stmt("delete from jobs where __seq = 1;"), &())
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_consumer_resolves_queue() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let schema = queue_schema("jobs");
        let queue_oid = schema.id();
        let bound = binder_arc(Arc::new(TestMetaMgr::new(schema)))
            .bind(parse_stmt("create consumer cg1 on jobs;"), &())
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::CreateConsumer(create)) = bound else {
            panic!("expected create consumer");
        };
        assert_eq!(create.table_id, queue_oid);
        assert_eq!(create.consumer, "cg1");
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_consumer_fails_on_missing_queue_and_plain_table() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(parse_stmt("create consumer cg1 on missing;"), &())
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);

        // `users` is a plain table, not a queue.
        let err = binder()
            .bind(parse_stmt("create consumer cg1 on users;"), &())
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_drop_consumer_resolves_queue() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let schema = queue_schema("jobs");
        let queue_oid = schema.id();
        let bound = binder_arc(Arc::new(TestMetaMgr::new(schema)))
            .bind(parse_stmt("drop consumer cg1 on jobs;"), &())
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::DropConsumer(drop)) = bound else {
            panic!("expected drop consumer");
        };
        assert_eq!(drop.table_id, queue_oid);
        assert_eq!(drop.consumer, "cg1");
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_drop_consumer_fails_when_missing() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(parse_stmt("drop consumer cg1 on missing;"), &())
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_receive_for_carries_consumer() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let schema = queue_schema_with_payload("jobs");
        let queue_oid = schema.id();
        let bound = binder_arc(Arc::new(TestMetaMgr::new(schema)))
            .bind(parse_stmt("receive 5 from jobs for cg1 wait 500;"), &())
            .await
            .unwrap();

        let BoundStmt::Query(BoundQuery::Receive(receive)) = bound else {
            panic!("expected receive query");
        };
        assert_eq!(receive.table_id, queue_oid);
        assert_eq!(receive.count, 5);
        assert!(receive.wait.is_some());
        assert_eq!(receive.consumer.as_deref(), Some("cg1"));
        let names: Vec<&str> = receive
            .tuple_desc
            .fields()
            .iter()
            .map(|field| field.name())
            .collect();
        assert_eq!(names, vec!["__seq", "__lsn", "body"]);

        // Without FOR there is no consumer cursor.
        let bound = binder_arc(Arc::new(TestMetaMgr::new(queue_schema_with_payload(
            "jobs",
        ))))
        .bind(parse_stmt("receive 5 from jobs;"), &())
        .await
        .unwrap();
        let BoundStmt::Query(BoundQuery::Receive(receive)) = bound else {
            panic!("expected receive query");
        };
        assert_eq!(receive.consumer, None);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_ack_for_carries_consumer() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let schema = queue_schema_with_payload("jobs");
        let queue_oid = schema.id();
        let bound = binder_arc(Arc::new(TestMetaMgr::new(schema)))
            .bind(parse_stmt("ack jobs for cg1 to 41;"), &())
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Ack(ack)) = bound else {
            panic!("expected ack command");
        };
        assert_eq!(ack.table_id, queue_oid);
        assert_eq!(ack.seq, 41);
        assert_eq!(ack.consumer.as_deref(), Some("cg1"));

        // Without FOR there is no consumer cursor.
        let bound = binder_arc(Arc::new(TestMetaMgr::new(queue_schema_with_payload(
            "jobs",
        ))))
        .bind(parse_stmt("ack jobs to 41;"), &())
        .await
        .unwrap();
        let BoundStmt::Command(BoundCommand::Ack(ack)) = bound else {
            panic!("expected ack command");
        };
        assert_eq!(ack.consumer, None);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_queue_with_partition_clause_binds_payload_column() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder_arc(Arc::new(TestMetaMgr::with_rule(
            queue_schema("jobs"),
            queue_i32_rule("r_q"),
        )))
        .bind(
            parse_stmt(
                "create queue events (region_id int, body text) \
                 partition by global rule r_q references (region_id);",
            ),
            &(),
        )
        .await
        .unwrap();

        let BoundStmt::Command(BoundCommand::CreateQueue(create)) = bound else {
            panic!("expected create queue");
        };
        let binding = create.partition_binding.as_ref().expect("binding");
        assert_eq!(binding.ref_attr_indices, vec![QUEUE_PAYLOAD_ATTR_START]);
        assert_eq!(binding.table_id, create.schema.id());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_queue_partition_rejects_bad_clauses() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let with_rule = || {
            binder_arc(Arc::new(TestMetaMgr::with_rule(
                queue_schema("jobs"),
                queue_i32_rule("r_q"),
            )))
        };
        // A system column cannot be the shard key.
        let err = with_rule()
            .bind(
                parse_stmt(
                    "create queue events (region_id int) \
                     partition by global rule r_q references (__seq);",
                ),
                &(),
            )
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        // Exactly one shard key column.
        let err = with_rule()
            .bind(
                parse_stmt(
                    "create queue events (region_id int, body text) \
                     partition by global rule r_q references (region_id, body);",
                ),
                &(),
            )
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        // The shard key type must match the rule key type.
        let err = with_rule()
            .bind(
                parse_stmt(
                    "create queue events (region_id int, body text) \
                     partition by global rule r_q references (body);",
                ),
                &(),
            )
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        // Unknown rule.
        let err = binder_arc(Arc::new(TestMetaMgr::new(queue_schema("jobs"))))
            .bind(
                parse_stmt(
                    "create queue events (region_id int) \
                     partition by global rule absent references (region_id);",
                ),
                &(),
            )
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    })
    .unwrap()
}
