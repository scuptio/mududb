//! Mixed absolute and delta assignments on the partition-local executor.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented
)]

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn update_applies_mixed_absolute_and_delta_assignments() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let contract = make_contract().await;
            // A table with three value columns so one UPDATE can mix an absolute
            // assignment with multiple delta assignments, like TPC-C payment.
            let schema = ::catalog::contract::schema_class::SchemaClass::new_with_oid(
                43,
                "t2".to_string(),
                ["id", "a", "b", "c"]
                    .into_iter()
                    .map(|name| {
                        SchemaColumn::new(
                            name.to_string(),
                            TypeFamily::I32,
                            mudu_type::data_type_codec::from_opt_object(&DataType::default_for(
                                TypeFamily::I32,
                            )),
                        )
                    })
                    .collect(),
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
        })
        .await;
}
