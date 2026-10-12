#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented
)]

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn build_cross_partition_tx_ops_groups_writes_uses_default_worker() {
    let data_dir = temp_dir()
        .join(format!("rpc_build_ops_{}", gen_oid()))
        .to_string_lossy()
        .to_string();
    let contract = WorkerXContract::with_log_and_data_dir(WorkerXContractParams {
        meta_mgr: Arc::new(TestMetaMgr::new()),
        log: None,
        log_layout: Default::default(),
        active_sessions: Default::default(),
        worker_id: 7,
        default_unpartitioned_worker_id: 7,
        partition_id: 0,
        data_dir,
        async_runtime: None,
        server_instance_id: 0,
    })
    .unwrap();

    let tx = contract.worker_begin_tx().unwrap();
    tx.put_relation(
        PhysicalRelationId {
            table_id: 1,
            partition_id: 0,
        },
        b"k0".to_vec(),
        b"v0".to_vec(),
    );
    tx.put_relation(
        PhysicalRelationId {
            table_id: 2,
            partition_id: 0,
        },
        b"k1".to_vec(),
        b"v1".to_vec(),
    );
    tx.put_relation(
        PhysicalRelationId {
            table_id: 3,
            partition_id: 1,
        },
        b"k2".to_vec(),
        b"v2".to_vec(),
    );
    tx.build_write_ops();

    let (participants, write_set) = contract
        .build_cross_partition_tx_ops(tx.as_ref())
        .await
        .unwrap();
    assert_eq!(write_set.len(), 3);
    let participant_by_partition: BTreeMap<OID, OID> = participants
        .into_iter()
        .map(|p| (p.partition_id, p.worker_id))
        .collect();
    assert_eq!(participant_by_partition.len(), 2);
    assert_eq!(participant_by_partition.get(&0).copied(), Some(7));
    assert_eq!(participant_by_partition.get(&1).copied(), Some(7));
}

#[tokio::test(flavor = "current_thread")]
async fn apply_cross_partition_ops_applies_local_writes() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    let key = build_key_tuple(&key_row(1), &desc).unwrap();
    let value = build_value_tuple(&value_row(10), &desc).unwrap();

    let participants = vec![CrossPartitionParticipant {
        partition_id: 0,
        worker_id: contract.worker_id(),
    }];
    let write_set = vec![XLWrite::Insert(XLInsert {
        table_id: table_id(),
        partition_id: 0,
        tuple_id: 0,
        key: key.clone(),
        value,
    })];

    contract
        .apply_cross_partition_ops(1, participants, write_set)
        .await
        .unwrap();

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
}

#[tokio::test(flavor = "current_thread")]
async fn apply_cross_partition_ops_sends_remote_request() {
    let bus = Arc::new(MockMessageBus::new(10));
    set_current_message_bus(bus.clone());
    let data_dir = temp_dir()
        .join(format!("rpc_remote_apply_{}", gen_oid()))
        .to_string_lossy()
        .to_string();
    let contract = WorkerXContract::with_log_and_data_dir(WorkerXContractParams {
        meta_mgr: Arc::new(TestMetaMgr::new()),
        log: None,
        log_layout: Default::default(),
        active_sessions: Default::default(),
        worker_id: 10,
        default_unpartitioned_worker_id: 20,
        partition_id: 0,
        data_dir,
        async_runtime: None,
        server_instance_id: 0,
    })
    .unwrap();

    bus.push_response(response_envelope(
        1,
        20,
        10,
        PartitionRpcResponse::ApplyCrossPartitionTx,
    ));

    let participants = vec![CrossPartitionParticipant {
        partition_id: 0,
        worker_id: 20,
    }];
    let write_set = vec![XLWrite::Insert(XLInsert {
        table_id: 1,
        partition_id: 0,
        tuple_id: 0,
        key: b"rk".to_vec(),
        value: b"rv".to_vec(),
    })];

    contract
        .apply_cross_partition_ops(1, participants, write_set.clone())
        .await
        .unwrap();

    let sent = bus.take_sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, 20);
    let decoded = rmp_serde::from_slice::<PartitionRpcRequest>(sent[0].1.payload()).unwrap();
    assert!(matches!(
        decoded,
        PartitionRpcRequest::ApplyCrossPartitionTx { .. }
    ));
    unset_current_message_bus();
}

#[tokio::test(flavor = "current_thread")]
async fn worker_commit_cross_partition_tx_async_commits_local_writes() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    let key1 = build_key_tuple(&key_row(1), &desc).unwrap();
    let value1 = build_value_tuple(&value_row(10), &desc).unwrap();
    let key2 = build_key_tuple(&key_row(2), &desc).unwrap();
    let value2 = build_value_tuple(&value_row(20), &desc).unwrap();

    let tx = contract.worker_begin_tx().unwrap();
    tx.put_relation(
        PhysicalRelationId {
            table_id: table_id(),
            partition_id: 0,
        },
        key1.clone(),
        value1,
    );
    tx.put_relation(
        PhysicalRelationId {
            table_id: table_id(),
            partition_id: 0,
        },
        key2.clone(),
        value2,
    );

    contract
        .worker_commit_cross_partition_tx_async(tx)
        .await
        .unwrap();

    for (key, expected) in [(key1, 10), (key2, 20)] {
        let read = PartitionRpcRequest::ReadKey {
            table_id: table_id(),
            partition_id: 0,
            key,
            select: vec![1],
        };
        let response = contract.execute_partition_rpc(read).await.unwrap();
        assert_eq!(
            response,
            PartitionRpcResponse::ReadKey(Some(vec![Some(datum(expected))]))
        );
    }
}

// 以下 handoff 测试用单进程 mock bus 近似双 worker：协调方 contract
// （worker 10，非分区表 owner 为 worker 20）与 owner contract（worker 20）
// 是两个独立实例，handoff 请求从 mock bus 捕获后交给 owner 实例执行。

#[tokio::test(flavor = "current_thread")]
async fn handoff_commit_single_remote_owner_applies_on_owner() {
    let bus = Arc::new(MockMessageBus::new(10));
    set_current_message_bus(bus.clone());
    let coordinator = make_contract_with_worker(10, 20).await;
    let owner = make_contract_with_worker(20, 20).await;

    let desc = meta_table(&test_schema()).unwrap();
    let key = build_key_tuple(&key_row(1), &desc).unwrap();
    let value = build_value_tuple(&value_row(10), &desc).unwrap();

    let tx = coordinator.worker_begin_tx().unwrap();
    tx.put_relation(
        PhysicalRelationId {
            table_id: table_id(),
            partition_id: 0,
        },
        key.clone(),
        value,
    );

    bus.push_response(response_envelope(
        1,
        20,
        10,
        PartitionRpcResponse::CommitWriteSet(1),
    ));
    coordinator.worker_commit_routed_tx_async(tx).await.unwrap();

    // The coordinator sent exactly one CommitWriteSet handoff to worker 20.
    let sent = bus.take_sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, 20);
    let request = rmp_serde::from_slice::<PartitionRpcRequest>(sent[0].1.payload()).unwrap();
    let PartitionRpcRequest::CommitWriteSet { writes, .. } = request else {
        panic!("expected CommitWriteSet request");
    };
    assert_eq!(writes.len(), 1);

    // The coordinator did not write the row locally.
    let read = PartitionRpcRequest::ReadKey {
        table_id: table_id(),
        partition_id: 0,
        key: key.clone(),
        select: vec![1],
    };
    let response = coordinator.execute_partition_rpc(read).await.unwrap();
    assert_eq!(response, PartitionRpcResponse::ReadKey(None));

    // The owner commits the handed-off write set through its normal local
    // commit path and the row becomes visible there.
    let response = owner
        .execute_partition_rpc(PartitionRpcRequest::CommitWriteSet {
            tx_id: 1,
            lock_token: None,
            writes,
        })
        .await
        .unwrap();
    assert_eq!(response, PartitionRpcResponse::CommitWriteSet(1));
    let read = PartitionRpcRequest::ReadKey {
        table_id: table_id(),
        partition_id: 0,
        key,
        select: vec![1],
    };
    let response = owner.execute_partition_rpc(read).await.unwrap();
    assert_eq!(
        response,
        PartitionRpcResponse::ReadKey(Some(vec![Some(datum(10))]))
    );
    unset_current_message_bus();
}

/// Regression test: a point read whose target partition is owned by
/// another worker must go to the owner even when this worker is worker 0.
/// The old `self.worker_id != 0` clause in the remote-branch guard made
/// worker 0 fall through to a local read against relation files it does
/// not host, returning an empty result for every remote-partition row.
#[tokio::test(flavor = "current_thread")]
async fn worker_zero_read_key_for_remote_owned_partition_goes_remote() {
    let bus = Arc::new(MockMessageBus::new(0));
    set_current_message_bus(bus.clone());
    // This contract IS worker 0; the partition resolves to worker 20.
    let contract = make_contract_with_worker(0, 20).await;

    let tx = contract.begin_tx().await.unwrap();
    bus.push_response(response_envelope(
        1,
        20,
        0,
        PartitionRpcResponse::ReadKey(Some(vec![Some(datum(10))])),
    ));
    let row = contract
        .read_key(
            tx.clone(),
            table_id(),
            &key_row(1),
            &VecSelTerm::new(vec![1]),
            &OptRead::default(),
        )
        .await
        .unwrap();
    assert_eq!(row, Some(vec![Some(datum(10))]));

    // Exactly one ReadKey RPC went to the owner, worker 20.
    let sent = bus.take_sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, 20);
    let request = rmp_serde::from_slice::<PartitionRpcRequest>(sent[0].1.payload()).unwrap();
    assert!(matches!(request, PartitionRpcRequest::ReadKey { .. }));
    unset_current_message_bus();
}

#[tokio::test(flavor = "current_thread")]
async fn handoff_commit_write_set_conflict_returns_error() {
    let bus = Arc::new(MockMessageBus::new(10));
    set_current_message_bus(bus.clone());
    let coordinator = make_contract_with_worker(10, 20).await;
    let owner = make_contract_with_worker(20, 20).await;

    let desc = meta_table(&test_schema()).unwrap();
    let key = build_key_tuple(&key_row(1), &desc).unwrap();
    let value = build_value_tuple(&value_row(10), &desc).unwrap();

    // Simulate a conflicting concurrent commit on the owner by holding the
    // commit lock for the same key from another transaction.
    let write_ops = vec![(
        PhysicalRelationId {
            table_id: table_id(),
            partition_id: 0,
        },
        key.clone(),
    )];
    assert!(owner.tx_lock.try_lock_some(999, &write_ops).unwrap());
    let conflict = owner
        .execute_partition_rpc(PartitionRpcRequest::CommitWriteSet {
            tx_id: 7,
            lock_token: None,
            writes: vec![XLWrite::Insert(XLInsert {
                table_id: table_id(),
                partition_id: 0,
                tuple_id: 0,
                key: key.clone(),
                value: value.clone(),
            })],
        })
        .await;
    assert!(conflict.is_err());
    owner.tx_lock.release(999, &write_ops).unwrap();

    // The conflict error travels back to the coordinator through the Err
    // response and fails the routed commit.
    let tx = coordinator.worker_begin_tx().unwrap();
    tx.put_relation(
        PhysicalRelationId {
            table_id: table_id(),
            partition_id: 0,
        },
        key,
        value,
    );
    bus.push_response(response_envelope(
        1,
        20,
        10,
        PartitionRpcResponse::Err(conflict.unwrap_err().to_string()),
    ));
    let result = coordinator.worker_commit_routed_tx_async(tx).await;
    assert!(result.is_err());
    unset_current_message_bus();
}

#[tokio::test(flavor = "current_thread")]
async fn remote_write_staging_read_your_writes() {
    let bus = Arc::new(MockMessageBus::new(10));
    set_current_message_bus(bus.clone());
    let contract = make_contract_with_worker(10, 20).await;

    let tx = contract.begin_tx().await.unwrap();
    // The statement-time existence check goes to the owner and reports the
    // key as missing, so the insert is staged locally.
    bus.push_response(response_envelope(
        1,
        20,
        10,
        PartitionRpcResponse::ReadKey(None),
    ));
    contract
        .insert(
            tx.clone(),
            table_id(),
            &key_row(1),
            &value_row(10),
            &OptInsert::default(),
        )
        .await
        .unwrap();

    // Read-your-writes: the staged overlay serves the read, no extra RPC.
    let row = contract
        .read_key(
            tx.clone(),
            table_id(),
            &key_row(1),
            &VecSelTerm::new(vec![1]),
            &OptRead::default(),
        )
        .await
        .unwrap();
    assert_eq!(row, Some(vec![Some(datum(10))]));
    assert_eq!(bus.take_sent().len(), 1);

    // A staged delete reads back as missing.
    let deleted = contract
        .delete(
            tx.clone(),
            table_id(),
            &key_row(1),
            &Predicate::CNF(vec![]),
            &OptDelete::default(),
        )
        .await
        .unwrap();
    assert_eq!(deleted, 1);
    let row = contract
        .read_key(
            tx.clone(),
            table_id(),
            &key_row(1),
            &VecSelTerm::new(vec![1]),
            &OptRead::default(),
        )
        .await
        .unwrap();
    assert_eq!(row, None);
    assert!(bus.take_sent().is_empty());

    // Re-inserting over the staged delete needs no remote existence check.
    contract
        .insert(
            tx.clone(),
            table_id(),
            &key_row(1),
            &value_row(30),
            &OptInsert::default(),
        )
        .await
        .unwrap();
    let row = contract
        .read_key(
            tx.clone(),
            table_id(),
            &key_row(1),
            &VecSelTerm::new(vec![1]),
            &OptRead::default(),
        )
        .await
        .unwrap();
    assert_eq!(row, Some(vec![Some(datum(30))]));
    assert!(bus.take_sent().is_empty());

    contract.abort_tx(tx).await.unwrap();
    unset_current_message_bus();
}

#[tokio::test(flavor = "current_thread")]
async fn remote_read_range_merges_staged_overlay() {
    let bus = Arc::new(MockMessageBus::new(10));
    set_current_message_bus(bus.clone());
    let contract = make_contract_with_worker(10, 20).await;

    let tx = contract.begin_tx().await.unwrap();
    // Stage an insert of key 5 (remote existence check: missing).
    bus.push_response(response_envelope(
        1,
        20,
        10,
        PartitionRpcResponse::ReadKey(None),
    ));
    contract
        .insert(
            tx.clone(),
            table_id(),
            &key_row(5),
            &value_row(50),
            &OptInsert::default(),
        )
        .await
        .unwrap();
    // Stage a delete of key 3 (remote existence check: present).
    bus.push_response(response_envelope(
        1,
        20,
        10,
        PartitionRpcResponse::ReadKey(Some(vec![])),
    ));
    let deleted = contract
        .delete(
            tx.clone(),
            table_id(),
            &key_row(3),
            &Predicate::CNF(vec![]),
            &OptDelete::default(),
        )
        .await
        .unwrap();
    assert_eq!(deleted, 1);

    // The owner still holds keys 1..=4; the select includes the key
    // attribute so the augmented-projection dedup path is exercised.
    let remote_rows: Vec<Vec<Option<Vec<u8>>>> = (1..=4)
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
            &Predicate::CNF(vec![]),
            &VecSelTerm::new(vec![0, 1]),
            &OptRead::default(),
        )
        .await
        .unwrap();

    let mut rows = Vec::new();
    while let Some(row) = cursor.next().await.unwrap() {
        rows.push((row.get(0).unwrap(), row.get(1).unwrap()));
    }
    // Key 3 is deleted by the staged overlay, key 5 is inserted by it.
    assert_eq!(
        rows,
        vec![
            (datum(1), datum(10)),
            (datum(2), datum(20)),
            (datum(4), datum(40)),
            (datum(5), datum(50)),
        ]
    );

    // One existence check per write plus one range scan were sent.
    assert_eq!(bus.take_sent().len(), 3);
    contract.abort_tx(tx).await.unwrap();
    unset_current_message_bus();
}
