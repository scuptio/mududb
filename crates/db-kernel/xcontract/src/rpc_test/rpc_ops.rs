#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented
)]

use super::*;

#[test]
fn cross_partition_wal_ops_empty_is_begin_commit() {
    let ops = cross_partition_wal_ops(&[]);
    assert_eq!(ops.len(), 2);
    assert!(matches!(ops[0], TxOp::Begin));
    assert!(matches!(ops[1], TxOp::Commit(_)));
}

#[test]
fn cross_partition_wal_ops_with_writes_preserves_order() {
    let writes = vec![
        XLWrite::Insert(XLInsert {
            table_id: 1,
            partition_id: 0,
            tuple_id: 0,
            key: b"a".to_vec(),
            value: b"1".to_vec(),
        }),
        XLWrite::Delete(XLDelete {
            table_id: 2,
            partition_id: 0,
            tuple_id: 0,
            key: b"b".to_vec(),
        }),
    ];
    let ops = cross_partition_wal_ops(&writes);
    assert_eq!(ops.len(), 4);
    assert!(matches!(ops[0], TxOp::Begin));
    assert!(matches!(ops[1], TxOp::Write(_)));
    assert!(matches!(ops[2], TxOp::Write(_)));
    assert!(matches!(ops[3], TxOp::Commit(_)));
}

#[test]
fn partition_write_set_filters_by_partition_id_preserving_order() {
    let writes = vec![
        XLWrite::Insert(XLInsert {
            table_id: 1,
            partition_id: 1,
            tuple_id: 0,
            key: b"a".to_vec(),
            value: b"1".to_vec(),
        }),
        XLWrite::Insert(XLInsert {
            table_id: 2,
            partition_id: 2,
            tuple_id: 0,
            key: b"b".to_vec(),
            value: b"2".to_vec(),
        }),
        XLWrite::Delete(XLDelete {
            table_id: 3,
            partition_id: 1,
            tuple_id: 0,
            key: b"c".to_vec(),
        }),
    ];
    let filtered = partition_write_set(&writes, 1);
    assert_eq!(filtered.len(), 2);
    assert_eq!(filtered[0].partition_id(), 1);
    assert_eq!(filtered[1].partition_id(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn execute_partition_rpc_read_key_missing() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    let key = build_key_tuple(&key_row(1), &desc).unwrap();
    let request = PartitionRpcRequest::ReadKey {
        table_id: table_id(),
        partition_id: 0,
        key,
        select: vec![1],
    };
    let response = contract.execute_partition_rpc(request).await.unwrap();
    assert_eq!(response, PartitionRpcResponse::ReadKey(None));
}

#[tokio::test(flavor = "current_thread")]
async fn execute_partition_rpc_insert_then_read_key() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    let key = build_key_tuple(&key_row(1), &desc).unwrap();
    let value = build_value_tuple(&value_row(10), &desc).unwrap();

    let insert = PartitionRpcRequest::Insert {
        table_id: table_id(),
        partition_id: 0,
        key: key.clone(),
        value: value.clone(),
    };
    assert_eq!(
        contract.execute_partition_rpc(insert).await.unwrap(),
        PartitionRpcResponse::Insert
    );

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
async fn execute_partition_rpc_insert_duplicate_returns_entity_already_exists() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    let key = build_key_tuple(&key_row(1), &desc).unwrap();
    let value = build_value_tuple(&value_row(10), &desc).unwrap();

    let insert = PartitionRpcRequest::Insert {
        table_id: table_id(),
        partition_id: 0,
        key: key.clone(),
        value: value.clone(),
    };
    contract
        .execute_partition_rpc(insert.clone())
        .await
        .unwrap();
    let result = contract.execute_partition_rpc(insert).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().ec(), ErrorCode::EntityAlreadyExists);
}

#[tokio::test(flavor = "current_thread")]
async fn execute_partition_rpc_delete_existing_and_missing() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    let key = build_key_tuple(&key_row(1), &desc).unwrap();
    let value = build_value_tuple(&value_row(10), &desc).unwrap();

    let insert = PartitionRpcRequest::Insert {
        table_id: table_id(),
        partition_id: 0,
        key: key.clone(),
        value,
    };
    contract.execute_partition_rpc(insert).await.unwrap();

    let delete = PartitionRpcRequest::Delete {
        table_id: table_id(),
        partition_id: 0,
        key: key.clone(),
    };
    assert_eq!(
        contract
            .execute_partition_rpc(delete.clone())
            .await
            .unwrap(),
        PartitionRpcResponse::Delete(1)
    );
    assert_eq!(
        contract.execute_partition_rpc(delete).await.unwrap(),
        PartitionRpcResponse::Delete(0)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn execute_partition_rpc_update_existing_and_missing() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    let key = build_key_tuple(&key_row(1), &desc).unwrap();
    let value = build_value_tuple(&value_row(10), &desc).unwrap();

    let update = PartitionRpcRequest::Update {
        table_id: table_id(),
        partition_id: 0,
        key: key.clone(),
        values: vec![(1, datum(20))],
    };
    assert_eq!(
        contract
            .execute_partition_rpc(update.clone())
            .await
            .unwrap(),
        PartitionRpcResponse::Update(0)
    );

    let insert = PartitionRpcRequest::Insert {
        table_id: table_id(),
        partition_id: 0,
        key: key.clone(),
        value,
    };
    contract.execute_partition_rpc(insert).await.unwrap();

    assert_eq!(
        contract.execute_partition_rpc(update).await.unwrap(),
        PartitionRpcResponse::Update(1)
    );

    let read = PartitionRpcRequest::ReadKey {
        table_id: table_id(),
        partition_id: 0,
        key,
        select: vec![1],
    };
    let response = contract.execute_partition_rpc(read).await.unwrap();
    assert_eq!(
        response,
        PartitionRpcResponse::ReadKey(Some(vec![Some(datum(20))]))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn execute_partition_rpc_read_range() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    let key1 = build_key_tuple(&key_row(1), &desc).unwrap();
    let key2 = build_key_tuple(&key_row(2), &desc).unwrap();
    let value1 = build_value_tuple(&value_row(10), &desc).unwrap();
    let value2 = build_value_tuple(&value_row(20), &desc).unwrap();

    for (key, value) in [(key1.clone(), value1), (key2.clone(), value2)] {
        let insert = PartitionRpcRequest::Insert {
            table_id: table_id(),
            partition_id: 0,
            key,
            value,
        };
        contract.execute_partition_rpc(insert).await.unwrap();
    }

    let request = PartitionRpcRequest::ReadRange {
        table_id: table_id(),
        partition_id: 0,
        start: RpcBound::Included(key1),
        end: RpcBound::Unbounded,
        select: vec![1],
    };
    let response = contract.execute_partition_rpc(request).await.unwrap();
    let PartitionRpcResponse::ReadRange(rows) = response else {
        panic!("expected ReadRange response");
    };
    assert_eq!(rows.len(), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn execute_partition_rpc_apply_cross_partition_tx() {
    let contract = make_contract().await;
    let desc = meta_table(&test_schema()).unwrap();
    let key = build_key_tuple(&key_row(1), &desc).unwrap();
    let value = build_value_tuple(&value_row(10), &desc).unwrap();

    let request = PartitionRpcRequest::ApplyCrossPartitionTx {
        tx_id: 42,
        coordinator_worker_id: 0,
        partition_id: 0,
        visibility_epoch: 0,
        partition_write_set: vec![XLWrite::Insert(XLInsert {
            table_id: table_id(),
            partition_id: 0,
            tuple_id: 0,
            key,
            value,
        })],
    };
    assert_eq!(
        contract.execute_partition_rpc(request).await.unwrap(),
        PartitionRpcResponse::ApplyCrossPartitionTx
    );
}

#[tokio::test(flavor = "current_thread")]
async fn send_partition_rpc_sends_request_and_decodes_response() {
    let bus = Arc::new(MockMessageBus::new(1));
    set_current_message_bus(bus.clone());
    let contract = make_contract().await;
    let response = PartitionRpcResponse::ReadKey(Some(vec![Some(b"v".to_vec())]));
    bus.push_response(response_envelope(1, 2, 0, response.clone()));

    let result = contract
        .send_partition_rpc(2, read_key_request())
        .await
        .unwrap();
    assert_eq!(result, response);

    let sent = bus.take_sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, 2);
    assert_eq!(sent[0].1.kind(), PARTITION_RPC_REQUEST_KIND);
    let decoded = rmp_serde::from_slice::<PartitionRpcRequest>(sent[0].1.payload()).unwrap();
    assert!(matches!(decoded, PartitionRpcRequest::ReadKey { .. }));

    unset_current_message_bus();
}

#[tokio::test(flavor = "current_thread")]
async fn send_partition_rpc_times_out() {
    let bus = Arc::new(MockMessageBus::new(1));
    set_current_message_bus(bus.clone());
    let contract = make_contract().await;

    let result = contract.send_partition_rpc(2, read_key_request()).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("timeout"));
    unset_current_message_bus();
}

#[tokio::test(flavor = "current_thread")]
async fn remote_read_key_matches_variant() {
    let bus = Arc::new(MockMessageBus::new(1));
    set_current_message_bus(bus.clone());
    let contract = make_contract().await;
    let response = PartitionRpcResponse::ReadKey(Some(vec![Some(b"v".to_vec())]));
    bus.push_response(response_envelope(1, 2, 0, response));

    let result = contract
        .remote_read_key(2, table_id(), 0, b"k".to_vec(), vec![])
        .await
        .unwrap();
    assert_eq!(result, Some(vec![Some(b"v".to_vec())]));
    unset_current_message_bus();
}

#[tokio::test(flavor = "current_thread")]
async fn remote_read_key_mismatches_variant() {
    let bus = Arc::new(MockMessageBus::new(1));
    set_current_message_bus(bus.clone());
    let contract = make_contract().await;
    bus.push_response(response_envelope(1, 2, 0, PartitionRpcResponse::Insert));

    let result = contract
        .remote_read_key(2, table_id(), 0, b"k".to_vec(), vec![])
        .await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("unexpected read_key rpc response"));
    unset_current_message_bus();
}

#[tokio::test(flavor = "current_thread")]
async fn handle_partition_rpc_sends_response() {
    let contract = make_contract().await;
    let bus = Arc::new(MockMessageBus::new(contract.worker_id()));
    set_current_message_bus(bus.clone());

    let request = read_key_request();
    let payload = rmp_serde::to_vec(&request).unwrap();
    let envelope = Envelope::new(
        1,
        None,
        2,
        contract.worker_id(),
        PARTITION_RPC_REQUEST_KIND,
        payload,
        DeliveryMode::Request,
    );
    contract.handle_partition_rpc(envelope).await.unwrap();

    let sent = bus.take_sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, 2);
    assert_eq!(sent[0].1.kind(), PARTITION_RPC_RESPONSE_KIND);
    assert_eq!(sent[0].1.correlation_id(), Some(1));
    let response = rmp_serde::from_slice::<PartitionRpcResponse>(sent[0].1.payload()).unwrap();
    assert!(matches!(response, PartitionRpcResponse::ReadKey(None)));
    unset_current_message_bus();
}

#[tokio::test(flavor = "current_thread")]
async fn handle_partition_rpc_without_bus_fails_entity_not_found() {
    unset_current_message_bus();
    let contract = make_contract().await;

    // Pre-insert a row so the execute step succeeds and the bus lookup is reached.
    let desc = meta_table(&test_schema()).unwrap();
    let key = build_key_tuple(&key_row(1), &desc).unwrap();
    let value = build_value_tuple(&value_row(10), &desc).unwrap();
    let insert = PartitionRpcRequest::Insert {
        table_id: table_id(),
        partition_id: 0,
        key: key.clone(),
        value,
    };
    contract.execute_partition_rpc(insert).await.unwrap();

    let read = PartitionRpcRequest::ReadKey {
        table_id: table_id(),
        partition_id: 0,
        key,
        select: vec![1],
    };
    let payload = rmp_serde::to_vec(&read).unwrap();
    let envelope = Envelope::new(
        1,
        None,
        2,
        contract.worker_id(),
        PARTITION_RPC_REQUEST_KIND,
        payload,
        DeliveryMode::Request,
    );
    let result = contract.handle_partition_rpc(envelope).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().ec(), ErrorCode::EntityNotFound);
}
