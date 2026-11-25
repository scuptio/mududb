#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented
)]

use super::*;

/// Builds a contract whose test table is bound to a two-partition rule,
/// both partitions placed on the local worker.
async fn make_partitioned_contract() -> (WorkerXContract, OID, OID, OID) {
    let contract = make_contract().await;
    let rule_id = 100;
    let parent_id = ::catalog::contract::derive_oid::derive_partition_oid("r_t", "p_lo");
    let child_id = ::catalog::contract::derive_oid::derive_partition_oid("r_t", "p_mid");
    let hi_id = ::catalog::contract::derive_oid::derive_partition_oid("r_t", "p_hi");
    let rule = ::catalog::contract::partition_rule::PartitionRuleDesc::new_range_with_oid(
        rule_id,
        "r_t".to_string(),
        vec![mudu_type::type_family::TypeFamily::I32],
        vec![
            ::catalog::contract::partition_rule::RangePartitionDef::new_with_oid(
                parent_id,
                "p_lo".to_string(),
                ::catalog::contract::partition_rule::PartitionBound::Unbounded,
                ::catalog::contract::partition_rule::PartitionBound::Value(vec![1000i32
                    .to_be_bytes()
                    .to_vec()]),
            ),
            ::catalog::contract::partition_rule::RangePartitionDef::new_with_oid(
                hi_id,
                "p_hi".to_string(),
                ::catalog::contract::partition_rule::PartitionBound::Value(vec![1000i32
                    .to_be_bytes()
                    .to_vec()]),
                ::catalog::contract::partition_rule::PartitionBound::Unbounded,
            ),
        ],
    );
    contract
        .meta_mgr()
        .create_partition_rule(&rule)
        .await
        .unwrap();
    contract
        .meta_mgr()
        .bind_table_partition(
            &::catalog::contract::partition_rule_binding::TablePartitionBinding {
                table_id: table_id(),
                rule_id,
                ref_attr_indices: vec![0],
            },
        )
        .await
        .unwrap();
    (contract, rule_id, parent_id, child_id)
}

#[tokio::test(flavor = "current_thread")]
async fn split_data_rehomes_rows_above_the_split_point() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (contract, rule_id, parent_id, child_id) = make_partitioned_contract().await;
            // Insert rows into the parent partition via the local write path.
            for (k, v) in [(1, 10), (499, 20), (500, 30), (900, 40)] {
                let tx = contract.begin_tx().await.unwrap();
                contract
                    .insert_on_partition(
                        tx.clone(),
                        table_id(),
                        parent_id,
                        &key_row(k),
                        &value_row(v),
                    )
                    .await
                    .unwrap();
                contract.commit_tx(tx).await.unwrap();
            }

            let response = contract
                .execute_partition_rpc(PartitionRpcRequest::PartitionSplitData {
                    partition_id: parent_id,
                    child_partition_id: child_id,
                    rule_id,
                    at: vec!["500".to_string()],
                })
                .await
                .unwrap();
            assert_eq!(
                response,
                PartitionRpcResponse::PartitionRehomed(2),
                "rows 500 and 900 must re-home"
            );
            for (k, v) in [(1, 10), (499, 20)] {
                assert!(
                    expect_local(&contract, parent_id, k, v).await,
                    "key {k} stays"
                );
            }
            for (k, v) in [(500, 30), (900, 40)] {
                assert!(
                    expect_local(&contract, child_id, k, v).await,
                    "key {k} moved"
                );
                assert!(expect_local_absent(&contract, parent_id, k).await);
            }
        })
        .await;
}

/// Whether the key is absent from the partition.
async fn expect_local_absent(contract: &WorkerXContract, partition_id: OID, k: i32) -> bool {
    let tx = contract.worker_begin_tx().unwrap();
    let value = contract
        .storage()
        .get_on_partition(
            table_id(),
            Some(partition_id),
            &build_key_tuple(&key_row(k), &meta_table(&test_schema()).unwrap()).unwrap(),
            tx.as_ref(),
        )
        .await
        .unwrap();
    contract.worker_rollback_tx(tx).unwrap();
    value.is_none()
}

/// Reads one key's committed value from a partition on the contract and
/// compares against the value tuple built for the row.
async fn expect_local(contract: &WorkerXContract, partition_id: OID, k: i32, v: i32) -> bool {
    let tx = contract.worker_begin_tx().unwrap();
    let value = contract
        .storage()
        .get_on_partition(
            table_id(),
            Some(partition_id),
            &build_key_tuple(&key_row(k), &meta_table(&test_schema()).unwrap()).unwrap(),
            tx.as_ref(),
        )
        .await
        .unwrap();
    contract.worker_rollback_tx(tx).unwrap();
    let expected = build_value_tuple(&value_row(v), &meta_table(&test_schema()).unwrap()).unwrap();
    value == Some(expected)
}

/// A stub raft share reporting "replica" (not the serving leader).
struct StubReplicaShare;

impl ::catalog::repl_ctrl::RaftShareView for StubReplicaShare {
    fn is_serving_leader(&self) -> bool {
        false
    }
    fn leader_hint(&self) -> Option<mudu_contract::protocol::LeaderHint> {
        None
    }
    fn leader_node_id(&self) -> Option<u64> {
        Some(1)
    }
    fn term_source(&self) -> Arc<std::sync::atomic::AtomicU64> {
        Arc::new(std::sync::atomic::AtomicU64::new(1))
    }
}

/// A stub cluster router: one group, self is node 2, partition p placed on
/// group 1 worker 0.
struct StubClusterRouter;

impl ::catalog::contract::cluster_router::ClusterRouter for StubClusterRouter {
    fn topology_version(&self) -> u64 {
        1
    }
    fn self_group_id(&self) -> u64 {
        1
    }
    fn group_generation(&self, _group_id: u64) -> RS<u64> {
        Ok(1)
    }
    fn self_node_id(&self) -> u64 {
        2
    }
    fn placement(&self, _rule: &str, _partition: &str) -> Option<(u64, u32)> {
        Some((1, 0))
    }
    fn resolve_group_worker(&self, _group_id: u64, _worker_index: u32) -> RS<OID> {
        Ok(0)
    }
    fn note_serving_node(&self, _group_id: u64, _node_id: u64) {}
    fn note_serving_failure(&self, _group_id: u64, _node_id: u64) {}
    fn serving_node(&self, _group_id: u64) -> Option<u64> {
        Some(1)
    }
    fn migration_state(
        &self,
        _rule: &str,
        _partition: &str,
    ) -> ::catalog::contract::cluster_router::MigrationState {
        ::catalog::contract::cluster_router::MigrationState::None
    }
    fn placement_version(&self) -> u64 {
        0
    }
}

#[tokio::test(flavor = "current_thread")]
async fn follower_read_serves_caught_up_replica_and_redirects_a_lagging_one() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let data_dir = temp_dir()
                .join(format!("rpc_follower_read_{}", gen_oid()))
                .to_string_lossy()
                .to_string();
            let oracle = crate::epoch_oracle::LocalEpochOracle::new_at(100);
            let contract = WorkerXContract::with_log_and_data_dir(WorkerXContractParams {
                meta_mgr: Arc::new(TestMetaMgr::new()),
                log: None,
                log_layout: Default::default(),
                active_sessions: Default::default(),
                worker_id: 2,
                default_unpartitioned_worker_id: 2,
                partition_id: 0,
                data_dir,
                async_runtime: None,
                server_instance_id: 0,
                cluster_router: Some(Arc::new(StubClusterRouter)),
                epoch_oracle: Some(oracle.clone()),
            })
            .unwrap();
            let schema = test_schema();
            let tx = contract.begin_tx().await.unwrap();
            contract.create_table(tx.clone(), &schema).await.unwrap();
            contract.commit_tx(tx).await.unwrap();
            // Write a row before the follower share installs.
            let tx = contract.begin_tx().await.unwrap();
            contract
                .insert_on_partition(tx.clone(), table_id(), 0, &key_row(1), &value_row(10))
                .await
                .unwrap();
            contract.commit_tx(tx).await.unwrap();
            contract.set_raft_share(Arc::new(StubReplicaShare)).unwrap();

            let read = PartitionRpcRequest::ReadKey {
                table_id: table_id(),
                partition_id: 0,
                key: build_key_tuple(&key_row(1), &meta_table(&test_schema()).unwrap()).unwrap(),
                select: vec![1],
            };
            use catalog::contract::epoch_oracle::EpochOracle;
            // A lagging replica (watermark 0 < stable epoch) redirects.
            let drained = oracle.register_active();
            oracle.unregister_active(drained);
            let response = contract
                .handle_partition_rpc_message(read.clone(), ServiceCtx::new(2, 111, 0))
                .await
                .unwrap();
            assert!(
                matches!(response, PartitionRpcResponse::Redirect { .. }),
                "lagging replica must redirect, got {response:?}"
            );
            // The replica caught up to the cluster stable epoch serves.
            let stable = oracle.stable_epoch();
            contract.snapshot_mgr.note_applied_commit_epoch(stable);
            let response = contract
                .handle_partition_rpc_message(read, ServiceCtx::new(2, 112, 0))
                .await
                .unwrap();
            assert!(
                matches!(response, PartitionRpcResponse::ReadKey(Some(_))),
                "caught-up replica must serve, got {response:?}"
            );
        })
        .await;
}
