#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented
)]

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn acquire_commit_locks_retries_until_holder_releases() {
    let mgr = Arc::new(XLockMgr::new());
    let relation = PhysicalRelationId {
        table_id: 1,
        partition_id: 0,
    };
    let ops = vec![(relation, b"k".to_vec())];
    assert!(mgr.try_lock_some(999, &ops).unwrap());

    let releaser = mgr.clone();
    let release_ops = ops.clone();
    tokio::spawn(async move {
        let _ = mudu_sys::task::async_::sleep(Duration::from_millis(5)).await;
        releaser.release(999, &release_ops).unwrap();
    });

    acquire_commit_locks(&mgr, 1000, &ops).await.unwrap();
    // The lock is now held by xid 1000; another owner cannot grab it.
    assert!(!mgr.try_lock_some(2000, &ops).unwrap());
    mgr.release(1000, &ops).unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn acquire_commit_locks_gives_up_after_bounded_attempts() {
    let mgr = XLockMgr::new();
    let relation = PhysicalRelationId {
        table_id: 1,
        partition_id: 0,
    };
    let ops = vec![(relation, b"k".to_vec())];
    assert!(mgr.try_lock_some(999, &ops).unwrap());

    let err = acquire_commit_locks(&mgr, 1000, &ops).await.unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Transaction);
    assert!(err.to_string().contains("failed to acquire commit locks"));
}

#[tokio::test(flavor = "current_thread")]
async fn routed_commit_all_local_behaves_like_local_commit() {
    let bus = Arc::new(MockMessageBus::new(0));
    set_current_message_bus(bus.clone());
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    let key = build_key_tuple(&key_row(1), &desc).unwrap();
    let value = build_value_tuple(&value_row(10), &desc).unwrap();

    let tx = contract.worker_begin_tx().unwrap();
    tx.put_relation(
        PhysicalRelationId {
            table_id: table_id(),
            partition_id: 0,
        },
        key.clone(),
        value,
    );
    contract.worker_commit_routed_tx_async(tx).await.unwrap();

    // A fully local commit sends no RPC at all.
    assert!(bus.take_sent().is_empty());

    let read = PartitionRpcRequest::ReadKey {
        table_id: table_id(),
        partition_id: 0,
        key,
        select: vec![1],
    };
    let response = contract.execute_partition_rpc(read).await.unwrap();
    assert_eq!(
        response,
        PartitionRpcResponse::ReadKey(Some(vec![Some(datum(10))]))
    );
    unset_current_message_bus();
}

#[tokio::test(flavor = "current_thread")]
async fn statement_lock_serializes_concurrent_updates() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    // Seed key 1 with value 10.
    let seed = PartitionRpcRequest::Insert {
        table_id: table_id(),
        partition_id: 0,
        key: build_key_tuple(&key_row(1), &desc).unwrap(),
        value: build_value_tuple(&value_row(10), &desc).unwrap(),
    };
    contract.execute_partition_rpc(seed).await.unwrap();

    // tx1 takes the statement-level write lock on key 1.
    let tx1 = contract.worker_begin_tx().unwrap();
    contract
        ._update(
            desc.clone(),
            tx1.clone(),
            table_id(),
            &key_row(1),
            &Predicate::CNF(vec![]),
            &VecDatum::new(vec![(1, datum(20))]),
            &[],
            &[],
        )
        .await
        .unwrap();

    // tx2's update of the same key must park on the statement lock and
    // proceed after tx1 commits — instead of failing at commit time.
    let tx2 = contract.worker_begin_tx().unwrap();
    let tx2_key = key_row(1);
    let tx2_pred = Predicate::CNF(vec![]);
    let tx2_values = VecDatum::new(vec![(1, datum(30))]);
    let tx2_update = contract._update(
        desc.clone(),
        tx2.clone(),
        table_id(),
        &tx2_key,
        &tx2_pred,
        &tx2_values,
        &[],
        &[],
    );
    let delayed_commit = async {
        let _ = mudu_sys::task::async_::sleep(Duration::from_millis(20)).await;
        contract.worker_commit_tx_async(tx1).await.unwrap();
    };
    let (tx2_result, ()) = futures::join!(tx2_update, delayed_commit);
    assert_eq!(tx2_result.unwrap(), 1);
    contract.worker_commit_tx_async(tx2).await.unwrap();

    let read = PartitionRpcRequest::ReadKey {
        table_id: table_id(),
        partition_id: 0,
        key: build_key_tuple(&key_row(1), &desc).unwrap(),
        select: vec![1],
    };
    let response = contract.execute_partition_rpc(read).await.unwrap();
    assert_eq!(
        response,
        PartitionRpcResponse::ReadKey(Some(vec![Some(datum(30))]))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn statement_lock_serializes_concurrent_delta_updates() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    // Seed key 1 with value 10.
    let seed = PartitionRpcRequest::Insert {
        table_id: table_id(),
        partition_id: 0,
        key: build_key_tuple(&key_row(1), &desc).unwrap(),
        value: build_value_tuple(&value_row(10), &desc).unwrap(),
    };
    contract.execute_partition_rpc(seed).await.unwrap();

    // tx1 takes the statement-level write lock on key 1 with `v = v + 1`.
    let tx1 = contract.worker_begin_tx().unwrap();
    let tx1_deltas = [DeltaAssign {
        attr: 1,
        op: DeltaOp::Add,
        literal: datum(1),
    }];
    contract
        ._update(
            desc.clone(),
            tx1.clone(),
            table_id(),
            &key_row(1),
            &Predicate::CNF(vec![]),
            &VecDatum::new(vec![]),
            &tx1_deltas,
            &[],
        )
        .await
        .unwrap();

    // tx2's `v = v + 1` parks on the statement lock; after tx1 commits it
    // must evaluate on the latest committed value (11), not on a stale
    // snapshot, so no increment is lost.
    let tx2 = contract.worker_begin_tx().unwrap();
    let tx2_deltas = [DeltaAssign {
        attr: 1,
        op: DeltaOp::Add,
        literal: datum(1),
    }];
    let tx2_values = VecDatum::new(vec![]);
    let tx2_pred = Predicate::CNF(vec![]);
    let tx2_key = key_row(1);
    let tx2_update = contract._update(
        desc.clone(),
        tx2.clone(),
        table_id(),
        &tx2_key,
        &tx2_pred,
        &tx2_values,
        &tx2_deltas,
        &[],
    );
    let delayed_commit = async {
        let _ = mudu_sys::task::async_::sleep(Duration::from_millis(20)).await;
        contract.worker_commit_tx_async(tx1).await.unwrap();
    };
    let (tx2_result, ()) = futures::join!(tx2_update, delayed_commit);
    assert_eq!(tx2_result.unwrap(), 1);
    contract.worker_commit_tx_async(tx2).await.unwrap();

    let read = PartitionRpcRequest::ReadKey {
        table_id: table_id(),
        partition_id: 0,
        key: build_key_tuple(&key_row(1), &desc).unwrap(),
        select: vec![1],
    };
    let response = contract.execute_partition_rpc(read).await.unwrap();
    assert_eq!(
        response,
        PartitionRpcResponse::ReadKey(Some(vec![Some(datum(12))]))
    );
}

/// Two deferred conditional-restock updates must NOT serialize on a
/// statement lock (the second update returns while the first transaction
/// is still open), and must still produce the same value as the
/// commutative form `((current - 10 - q) mod 91) + 10` in either apply
/// order.
#[tokio::test(flavor = "current_thread")]
async fn deferred_sub_wrap_updates_are_lockfree_and_commute() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    // Seed key 1 with stock 50.
    let seed = PartitionRpcRequest::Insert {
        table_id: table_id(),
        partition_id: 0,
        key: build_key_tuple(&key_row(1), &desc).unwrap(),
        value: build_value_tuple(&value_row(50), &desc).unwrap(),
    };
    contract.execute_partition_rpc(seed).await.unwrap();

    let wrap_delta = |quantity: i64| DeltaAssign {
        attr: 1,
        op: DeltaOp::SubWrapDeferred,
        literal: crate::utils::encode_sub_wrap_literal(quantity, 10, 91),
    };
    // tx1 stages a restock update but stays open; tx2's update must NOT
    // park on a statement lock (lock-free path).
    let tx1 = contract.worker_begin_tx().unwrap();
    contract
        ._update(
            desc.clone(),
            tx1.clone(),
            table_id(),
            &key_row(1),
            &Predicate::CNF(vec![]),
            &VecDatum::new(vec![]),
            &[wrap_delta(5)],
            &[],
        )
        .await
        .unwrap();
    let tx2 = contract.worker_begin_tx().unwrap();
    let tx2_key = key_row(1);
    let tx2_pred = Predicate::CNF(vec![]);
    let tx2_values = VecDatum::new(vec![]);
    let tx2_deltas = [wrap_delta(85)];
    let tx2_update = contract._update(
        desc.clone(),
        tx2.clone(),
        table_id(),
        &tx2_key,
        &tx2_pred,
        &tx2_values,
        &tx2_deltas,
        &[],
    );
    let updated = mudu_sys::task::async_::timeout(Duration::from_millis(500), tx2_update)
        .await
        .expect("deferred update must not block on a statement lock")
        .unwrap();
    assert_eq!(updated, 1);

    // Commit tx2 before tx1: apply order is 85-then-5. 50 -> ((50-95) mod
    // 91)+10 = 56, then 56 -> ((56-15) mod 91)+10 = 51. The other order
    // (5 then 85: 50 -> 45 -> 51) yields the same value.
    contract.worker_commit_tx_async(tx2).await.unwrap();
    contract.worker_commit_tx_async(tx1).await.unwrap();
    let read = PartitionRpcRequest::ReadKey {
        table_id: table_id(),
        partition_id: 0,
        key: build_key_tuple(&key_row(1), &desc).unwrap(),
        select: vec![1],
    };
    let response = contract.execute_partition_rpc(read).await.unwrap();
    assert_eq!(
        response,
        PartitionRpcResponse::ReadKey(Some(vec![Some(datum(51))]))
    );
}

/// Deferred add/sub increments from concurrent transactions all apply on
/// top of each other (no lost update) without a statement lock, and a
/// point read inside the transaction observes its own staged deferred
/// delta (read-your-writes fold).
#[tokio::test(flavor = "current_thread")]
async fn deferred_add_accumulates_lockfree_and_reads_your_writes() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    let seed = PartitionRpcRequest::Insert {
        table_id: table_id(),
        partition_id: 0,
        key: build_key_tuple(&key_row(1), &desc).unwrap(),
        value: build_value_tuple(&value_row(10), &desc).unwrap(),
    };
    contract.execute_partition_rpc(seed).await.unwrap();

    let add_deferred = |attr, v: i32| DeltaAssign {
        attr,
        op: DeltaOp::AddDeferred,
        literal: datum(v),
    };
    let tx1 = contract.worker_begin_tx().unwrap();
    contract
        ._update(
            desc.clone(),
            tx1.clone(),
            table_id(),
            &key_row(1),
            &Predicate::CNF(vec![]),
            &VecDatum::new(vec![]),
            &[add_deferred(1, 1)],
            &[],
        )
        .await
        .unwrap();
    // Read-your-writes: the staged deferred delta folds over the
    // committed value (10 + 1) without being applied yet.
    let row = contract
        .read_key(
            tx1.clone(),
            table_id(),
            &key_row(1),
            &VecSelTerm::new(vec![1]),
            &OptRead::default(),
        )
        .await
        .unwrap();
    assert_eq!(row, Some(vec![Some(datum(11))]));

    let tx2 = contract.worker_begin_tx().unwrap();
    let tx2_key = key_row(1);
    let tx2_pred = Predicate::CNF(vec![]);
    let tx2_values = VecDatum::new(vec![]);
    let tx2_deltas = [add_deferred(1, 2)];
    let tx2_update = contract._update(
        desc.clone(),
        tx2.clone(),
        table_id(),
        &tx2_key,
        &tx2_pred,
        &tx2_values,
        &tx2_deltas,
        &[],
    );
    mudu_sys::task::async_::timeout(Duration::from_millis(500), tx2_update)
        .await
        .expect("deferred update must not block on a statement lock")
        .unwrap();

    contract.worker_commit_tx_async(tx2).await.unwrap();
    contract.worker_commit_tx_async(tx1).await.unwrap();
    let read = PartitionRpcRequest::ReadKey {
        table_id: table_id(),
        partition_id: 0,
        key: build_key_tuple(&key_row(1), &desc).unwrap(),
        select: vec![1],
    };
    let response = contract.execute_partition_rpc(read).await.unwrap();
    assert_eq!(
        response,
        PartitionRpcResponse::ReadKey(Some(vec![Some(datum(13))]))
    );
}

/// Deferred deltas cannot be mixed with absolute assignments in one call.
#[tokio::test(flavor = "current_thread")]
async fn deferred_delta_mixed_with_absolute_assignment_is_rejected() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    let seed = PartitionRpcRequest::Insert {
        table_id: table_id(),
        partition_id: 0,
        key: build_key_tuple(&key_row(1), &desc).unwrap(),
        value: build_value_tuple(&value_row(10), &desc).unwrap(),
    };
    contract.execute_partition_rpc(seed).await.unwrap();

    let tx = contract.worker_begin_tx().unwrap();
    let result = contract
        ._update(
            desc.clone(),
            tx.clone(),
            table_id(),
            &key_row(1),
            &Predicate::CNF(vec![]),
            &value_row(99),
            &[DeltaAssign {
                attr: 1,
                op: DeltaOp::AddDeferred,
                literal: datum(1),
            }],
            &[],
        )
        .await;
    assert!(result.is_err());
}

/// The conditional-restock evaluation is exactly `((x - 10 - q) mod 91) + 10`:
/// threshold boundary respected, and any two restock updates commute.
#[test]
fn sub_wrap_eval_commutes_and_respects_threshold() {
    let desc = meta_table(&test_schema()).unwrap();
    let eval = |current: i32, q: i64| -> i32 {
        let updated = apply_value_update_with_deltas(
            &build_value_tuple(&value_row(current), &desc).unwrap(),
            &VecDatum::new(vec![]),
            &[DeltaAssign {
                attr: 1,
                op: DeltaOp::SubWrapDeferred,
                literal: crate::utils::encode_sub_wrap_literal(q, 10, 91),
            }],
            &[],
            &desc,
        )
        .unwrap();
        let field = desc.value_desc().get_field_desc(0).get(&updated).unwrap();
        i32::from_be_bytes(field.try_into().unwrap())
    };
    // Boundary: 50 - 40 = 10 is NOT below 10, so no restock wrap.
    assert_eq!(eval(50, 40), 10);
    // 50 - 41 = 9 < 10 triggers the wrap: 9 + 91 = 100.
    assert_eq!(eval(50, 41), 100);
    // f5(f85(x)) == f85(f5(x)) for the boundary-crossing case too.
    assert_eq!(eval(eval(50, 85), 5), eval(eval(50, 5), 85));
    // wrap <= floor is rejected.
    let rejected = apply_value_update_with_deltas(
        &build_value_tuple(&value_row(50), &desc).unwrap(),
        &VecDatum::new(vec![]),
        &[DeltaAssign {
            attr: 1,
            op: DeltaOp::SubWrapDeferred,
            literal: crate::utils::encode_sub_wrap_literal(5, 10, 10),
        }],
        &[],
        &desc,
    );
    assert!(rejected.is_err());
}

/// Bind `v = v + ?` through the real SQL binder with the amount supplied
/// as a parameter, returning the resulting delta assignment.
async fn bind_parameterized_delta(sql: &str, params: &(i32, i32)) -> DeltaAssign {
    let meta_mgr = Arc::new(TestMetaMgr::new());
    ::catalog::contract::meta_mgr::MetaMgr::create_table(meta_mgr.as_ref(), &test_schema())
        .await
        .unwrap();
    let binder = ::sql::binder::Binder::new(
        meta_mgr,
        ::catalog::contract::app_schema::DEFAULT_SCHEMA.to_string(),
    );
    let stmt = sql_parser::ast::parser::SQLParser::new()
        .unwrap()
        .parse(sql)
        .unwrap()
        .stmts()[0]
        .clone();
    let bound = binder.bind(stmt, params).await.unwrap();
    let ::sql::bound_stmt::BoundStmt::Command(::sql::bound_stmt::BoundCommand::Update(update)) =
        bound
    else {
        panic!("expected bound update");
    };
    assert_eq!(update.value.len(), 1);
    let ::sql::bound_stmt::BoundSetValue::Delta { op, literal } = &update.value[0].1 else {
        panic!("expected delta assignment, got {:?}", update.value[0].1);
    };
    DeltaAssign {
        attr: update.value[0].0,
        op: *op,
        literal: literal.clone(),
    }
}

// Miri cannot execute FFI calls into the tree-sitter C parser used by
// SQLParser inside `bind_parameterized_delta`; skipped under Miri.
#[tokio::test(flavor = "current_thread")]
#[cfg_attr(miri, ignore)]
async fn statement_lock_serializes_concurrent_parameterized_delta_updates() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    // Seed key 1 with value 100.
    let seed = PartitionRpcRequest::Insert {
        table_id: table_id(),
        partition_id: 0,
        key: build_key_tuple(&key_row(1), &desc).unwrap(),
        value: build_value_tuple(&value_row(100), &desc).unwrap(),
    };
    contract.execute_partition_rpc(seed).await.unwrap();

    // Two transactions each run a payment-style `v = v + ?` with amount 7
    // supplied as a parameter.
    let tx1_deltas =
        [bind_parameterized_delta("update t set v = v + ? where id = ?;", &(7i32, 1i32)).await];
    let tx1 = contract.worker_begin_tx().unwrap();
    contract
        ._update(
            desc.clone(),
            tx1.clone(),
            table_id(),
            &key_row(1),
            &Predicate::CNF(vec![]),
            &VecDatum::new(vec![]),
            &tx1_deltas,
            &[],
        )
        .await
        .unwrap();

    // tx2's parameterized delta parks on the statement lock; after tx1
    // commits it must evaluate on the latest committed value (107), so the
    // final value is 100 + 2 * 7 and no increment is lost.
    let tx2_deltas =
        [bind_parameterized_delta("update t set v = v + ? where id = ?;", &(7i32, 1i32)).await];
    let tx2 = contract.worker_begin_tx().unwrap();
    let tx2_key = key_row(1);
    let tx2_pred = Predicate::CNF(vec![]);
    let tx2_values = VecDatum::new(vec![]);
    let tx2_update = contract._update(
        desc.clone(),
        tx2.clone(),
        table_id(),
        &tx2_key,
        &tx2_pred,
        &tx2_values,
        &tx2_deltas,
        &[],
    );
    let delayed_commit = async {
        let _ = mudu_sys::task::async_::sleep(Duration::from_millis(20)).await;
        contract.worker_commit_tx_async(tx1).await.unwrap();
    };
    let (tx2_result, ()) = futures::join!(tx2_update, delayed_commit);
    assert_eq!(tx2_result.unwrap(), 1);
    contract.worker_commit_tx_async(tx2).await.unwrap();

    let read = PartitionRpcRequest::ReadKey {
        table_id: table_id(),
        partition_id: 0,
        key: build_key_tuple(&key_row(1), &desc).unwrap(),
        select: vec![1],
    };
    let response = contract.execute_partition_rpc(read).await.unwrap();
    assert_eq!(
        response,
        PartitionRpcResponse::ReadKey(Some(vec![Some(datum(114))]))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn update_applies_mixed_absolute_and_delta_assignments() {
    let contract = make_contract().await;
    // A table with three value columns so one UPDATE can mix an absolute
    // assignment with multiple delta assignments, like TPC-C payment.
    let schema = ::catalog::contract::schema_table::SchemaTable::new_with_oid(
        43,
        "t2".to_string(),
        vec![
            SchemaColumn::new(
                "id".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
            SchemaColumn::new(
                "a".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
            SchemaColumn::new(
                "b".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
            SchemaColumn::new(
                "c".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
        ],
        vec![0],
        vec![1, 2, 3],
    );
    let desc = meta_table(&schema).unwrap();
    let tx = contract.begin_tx().await.unwrap();
    contract.create_table(tx.clone(), &schema).await.unwrap();
    contract.commit_tx(tx).await.unwrap();

    // Seed key 1 with (a, b, c) = (1, 100, 1000).
    let seed = PartitionRpcRequest::Insert {
        table_id: schema.id(),
        partition_id: 0,
        key: build_key_tuple(&key_row(1), &desc).unwrap(),
        value: build_value_tuple(
            &VecDatum::new(vec![(1, datum(1)), (2, datum(100)), (3, datum(1000))]),
            &desc,
        )
        .unwrap(),
    };
    contract.execute_partition_rpc(seed).await.unwrap();

    // One statement: a = 7 (absolute), b = b + 5 (delta), c = c - 30
    // (delta).
    let tx = contract.worker_begin_tx().unwrap();
    let updated = contract
        ._update(
            desc.clone(),
            tx.clone(),
            schema.id(),
            &key_row(1),
            &Predicate::CNF(vec![]),
            &VecDatum::new(vec![(1, datum(7))]),
            &[
                DeltaAssign {
                    attr: 2,
                    op: DeltaOp::Add,
                    literal: datum(5),
                },
                DeltaAssign {
                    attr: 3,
                    op: DeltaOp::Sub,
                    literal: datum(30),
                },
            ],
            &[],
        )
        .await
        .unwrap();
    assert_eq!(updated, 1);
    contract.worker_commit_tx_async(tx).await.unwrap();

    let read = PartitionRpcRequest::ReadKey {
        table_id: schema.id(),
        partition_id: 0,
        key: build_key_tuple(&key_row(1), &desc).unwrap(),
        select: vec![1, 2, 3],
    };
    let response = contract.execute_partition_rpc(read).await.unwrap();
    assert_eq!(
        response,
        PartitionRpcResponse::ReadKey(Some(vec![
            Some(datum(7)),
            Some(datum(105)),
            Some(datum(970))
        ]))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn statement_lock_cross_update_deadlock_resolves_by_timeout() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    for (k, v) in [(1, 10), (2, 20)] {
        let seed = PartitionRpcRequest::Insert {
            table_id: table_id(),
            partition_id: 0,
            key: build_key_tuple(&key_row(k), &desc).unwrap(),
            value: build_value_tuple(&value_row(v), &desc).unwrap(),
        };
        contract.execute_partition_rpc(seed).await.unwrap();
    }

    // tx1 locks k1, tx2 locks k2; each then tries the other's key.
    let tx1 = contract.worker_begin_tx().unwrap();
    contract
        ._update(
            desc.clone(),
            tx1.clone(),
            table_id(),
            &key_row(1),
            &Predicate::CNF(vec![]),
            &VecDatum::new(vec![(1, datum(11))]),
            &[],
            &[],
        )
        .await
        .unwrap();
    let tx2 = contract.worker_begin_tx().unwrap();
    contract
        ._update(
            desc.clone(),
            tx2.clone(),
            table_id(),
            &key_row(2),
            &Predicate::CNF(vec![]),
            &VecDatum::new(vec![(1, datum(21))]),
            &[],
            &[],
        )
        .await
        .unwrap();

    let wait1_key = key_row(2);
    let wait1_pred = Predicate::CNF(vec![]);
    let wait1_values = VecDatum::new(vec![(1, datum(22))]);
    let wait1 = contract._update(
        desc.clone(),
        tx1.clone(),
        table_id(),
        &wait1_key,
        &wait1_pred,
        &wait1_values,
        &[],
        &[],
    );
    let wait2_key = key_row(1);
    let wait2_pred = Predicate::CNF(vec![]);
    let wait2_values = VecDatum::new(vec![(1, datum(12))]);
    let wait2 = contract._update(
        desc.clone(),
        tx2.clone(),
        table_id(),
        &wait2_key,
        &wait2_pred,
        &wait2_values,
        &[],
        &[],
    );
    let started = *mudu_sys::time::instant_now();
    let (r1, r2) = futures::join!(wait1, wait2);
    // The circular wait is broken by the bounded statement-lock wait
    // (STATEMENT_LOCK_TIMEOUT), never by a hang; at least one side
    // reports the Transaction error.
    assert!(started.elapsed() < STATEMENT_LOCK_TIMEOUT * 2);
    assert!(
        r1.as_ref()
            .err()
            .is_some_and(|e| e.ec() == ErrorCode::Transaction)
            || r2
                .as_ref()
                .err()
                .is_some_and(|e| e.ec() == ErrorCode::Transaction),
        "expected a transaction timeout on one side: {r1:?} {r2:?}"
    );
    let _ = contract.worker_abort_tx_async(tx1).await;
    let _ = contract.worker_abort_tx_async(tx2).await;
}

#[tokio::test(flavor = "current_thread")]
async fn statement_lock_released_on_rollback() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    let seed = PartitionRpcRequest::Insert {
        table_id: table_id(),
        partition_id: 0,
        key: build_key_tuple(&key_row(1), &desc).unwrap(),
        value: build_value_tuple(&value_row(10), &desc).unwrap(),
    };
    contract.execute_partition_rpc(seed).await.unwrap();

    let tx1 = contract.worker_begin_tx().unwrap();
    contract
        ._update(
            desc.clone(),
            tx1.clone(),
            table_id(),
            &key_row(1),
            &Predicate::CNF(vec![]),
            &VecDatum::new(vec![(1, datum(20))]),
            &[],
            &[],
        )
        .await
        .unwrap();
    contract.worker_abort_tx_async(tx1).await.unwrap();

    // The rollback released the statement lock: tx2 takes it without
    // waiting (far below the 25ms timeout).
    let tx2 = contract.worker_begin_tx().unwrap();
    let acquired = mudu_sys::task::async_::timeout(
        Duration::from_millis(500),
        contract._update(
            desc.clone(),
            tx2.clone(),
            table_id(),
            &key_row(1),
            &Predicate::CNF(vec![]),
            &VecDatum::new(vec![(1, datum(30))]),
            &[],
            &[],
        ),
    )
    .await
    .expect("update after rollback must not park");
    assert_eq!(acquired.unwrap(), 1);
    let _ = contract.worker_abort_tx_async(tx2).await;
}

#[tokio::test(flavor = "current_thread")]
async fn remote_read_range_key_prefix_eq_filters_rows() {
    let bus = Arc::new(MockMessageBus::new(10));
    set_current_message_bus(bus.clone());
    let contract = make_contract_with_worker(10, 20).await;

    let tx = contract.begin_tx().await.unwrap();
    // The owner returns rows with the augmented projection (key attrs
    // first); the key-prefix predicate must filter them client-side.
    let remote_rows: Vec<Vec<Option<Vec<u8>>>> = (1..=3)
        .map(|i| vec![Some(datum(i)), Some(datum(i * 10))])
        .collect();
    bus.push_response(response_envelope(
        1,
        20,
        10,
        PartitionRpcResponse::ReadRange(remote_rows),
    ));
    let cursor = contract
        .read_range(
            tx.clone(),
            table_id(),
            &RangeData::new(
                std::ops::Bound::Included(vec![(0, datum(1))]),
                std::ops::Bound::Unbounded,
            ),
            &Predicate::KeyPrefixEq(vec![(0, datum(1))]),
            &VecSelTerm::new(vec![0, 1]),
            &OptRead::default(),
        )
        .await
        .unwrap();

    let mut rows = Vec::new();
    while let Some(row) = cursor.next().await.unwrap() {
        rows.push((row.get(0).unwrap(), row.get(1).unwrap()));
    }
    assert_eq!(rows, vec![(datum(1), datum(10))]);
    contract.abort_tx(tx).await.unwrap();
    unset_current_message_bus();
}

/// Replay the captured `LockKeyForUpdate` against the real owner so its lock
/// table reflects the coordinator's statement token, and verify the owner
/// then holds the token's lock on the key. Returns the locked relation id
/// and encoded key.
async fn replay_lock_on_owner(
    owner: &WorkerXContract,
    bus: &Arc<MockMessageBus>,
    token: OID,
) -> (PhysicalRelationId, Vec<u8>) {
    let sent = bus.take_sent();
    assert_eq!(sent.len(), 1);
    let request = rmp_serde::from_slice::<PartitionRpcRequest>(sent[0].1.payload()).unwrap();
    let PartitionRpcRequest::LockKeyForUpdate {
        lock_token,
        table_id: req_table,
        partition_id: req_partition,
        key: req_key,
        ..
    } = request
    else {
        panic!("expected LockKeyForUpdate request");
    };
    assert_eq!(lock_token, token);
    let relation_id = PhysicalRelationId {
        table_id: req_table,
        partition_id: req_partition,
    };
    let owner_response = owner
        .execute_partition_rpc(PartitionRpcRequest::LockKeyForUpdate {
            lock_token,
            table_id: req_table,
            partition_id: req_partition,
            key: req_key.clone(),
            select: vec![1],
        })
        .await
        .unwrap();
    assert_eq!(
        owner_response,
        PartitionRpcResponse::ReadKey(Some(vec![Some(datum(10))]))
    );
    // The owner now holds the token's lock on the key.
    assert!(!owner
        .tx_lock
        .try_lock_some(999, &[(relation_id, req_key.clone())])
        .unwrap());
    (relation_id, req_key)
}

/// Hand off the commit to the owner: the `CommitWriteSet` request carries
/// the coordinator's statement token, and the owner commits under the held
/// lock, releasing all of the token's locks afterwards.
async fn handoff_commit_on_owner(
    coordinator: &WorkerXContract,
    owner: &WorkerXContract,
    bus: &Arc<MockMessageBus>,
    tx: Arc<dyn TxMgr>,
    token: OID,
) {
    bus.push_response(response_envelope(
        1,
        20,
        10,
        PartitionRpcResponse::CommitWriteSet(1),
    ));
    coordinator.worker_commit_routed_tx_async(tx).await.unwrap();
    let sent = bus.take_sent();
    assert_eq!(sent.len(), 1);
    let request = rmp_serde::from_slice::<PartitionRpcRequest>(sent[0].1.payload()).unwrap();
    let PartitionRpcRequest::CommitWriteSet {
        lock_token: commit_token,
        writes,
        ..
    } = request
    else {
        panic!("expected CommitWriteSet request");
    };
    assert_eq!(commit_token, Some(token));
    let response = owner
        .execute_partition_rpc(PartitionRpcRequest::CommitWriteSet {
            tx_id: 1,
            lock_token: commit_token,
            writes,
        })
        .await
        .unwrap();
    assert_eq!(response, PartitionRpcResponse::CommitWriteSet(1));
}

#[tokio::test(flavor = "current_thread")]
async fn remote_statement_lock_and_handoff_commit_full_lifecycle() {
    let bus = Arc::new(MockMessageBus::new(10));
    set_current_message_bus(bus.clone());
    let coordinator = make_contract_with_worker(10, 20).await;
    let owner = make_contract_with_worker(20, 20).await;
    let desc = meta_table(&test_schema()).unwrap();

    // Seed the row on the owner.
    seed_owner_key1(&owner, &desc).await;

    // The coordinator's update sends LockKeyForUpdate; the owner worker is
    // recorded for rollback release.
    let tx = coordinator_update_key1(&coordinator, &bus, &desc).await;
    let token = statement_lock_token(10, tx.xid());

    // The coordinator locked via LockKeyForUpdate carrying its statement
    // token; replay that request against the real owner so its lock
    // table reflects the lock.
    let (relation_id, req_key) = replay_lock_on_owner(&owner, &bus, token).await;

    // Handoff commit carries the token; the owner commits under the
    // held lock and releases all of the token's locks afterwards.
    handoff_commit_on_owner(&coordinator, &owner, &bus, tx, token).await;

    // Committed value visible on the owner, and the token's locks are
    // all released.
    assert!(owner
        .tx_lock
        .try_lock_some(999, &[(relation_id, req_key.clone())])
        .unwrap());
    let read = PartitionRpcRequest::ReadKey {
        table_id: table_id(),
        partition_id: 0,
        key: req_key,
        select: vec![1],
    };
    let response = owner.execute_partition_rpc(read).await.unwrap();
    assert_eq!(
        response,
        PartitionRpcResponse::ReadKey(Some(vec![Some(datum(30))]))
    );
    unset_current_message_bus();
}
