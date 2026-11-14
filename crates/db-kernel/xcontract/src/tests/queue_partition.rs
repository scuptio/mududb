#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Sharded-queue tests over a real `WorkerXContract` and real relation
//! storage: a queue created with `PARTITION BY GLOBAL RULE ... REFERENCES
//! (<col>)` shards its rows by the payload column; both shards are placed on
//! the single test worker (the cross-worker transport paths ride the
//! partition RPC machinery covered by `rpc_test`). Covers keyed
//! distribution, per-shard sequence spaces, round-robin drain, per-shard
//! ACK, and the per-shard runtime rebuild after a reopen.

use super::block_on;
use ::catalog::contract::queue::queue_seq_shard;
use mudu_type::data_type::DataType;
use mudu_type::type_family::TypeFamily;

/// The `body` payload field of an envelope row of the two-payload-column
/// queue (fields: `__seq`, `__lsn`, `region`, `body`).
fn row_body3(row: &TupleRow) -> String {
    use mudu_type::datum::DatumDyn;
    mudu_contract::tuple::typed_bin::TypedBin::new(
        TypeFamily::String,
        row.fields()[3].as_ref().unwrap().clone(),
    )
    .to_value(&DataType::default_for(TypeFamily::String))
    .unwrap()
    .into_string()
}
use super::queue::{make_contract, row_seq, run_command, run_query, temp_data_dir};
use crate::*;
use ::catalog_testing::test_meta_mgr::TestMetaMgr;

/// Creates the two-partition rule `r_sq` over an I32 payload column and
/// places both partitions on the test worker (id 0).
async fn shard_rule(contract: &Arc<WorkerXContract>, meta: Arc<TestMetaMgr>) -> RS<()> {
    run_command(
        contract,
        meta.clone(),
        "create partition rule r_sq range ( \
            partition p0 values from (minvalue) to (100), \
            partition p1 values from (100) to (maxvalue) \
        );",
    )
    .await?;
    let rule = meta.get_partition_rule_by_name("r_sq").await?.unwrap();
    for partition in &rule.partitions {
        meta.place_partition(partition.partition_id, 0);
    }
    Ok(())
}

#[test]
fn sharded_queue_routes_by_key_and_drains_per_shard() {
    block_on(async move {
        let meta = Arc::new(TestMetaMgr::new());
        let contract = make_contract(meta.clone(), temp_data_dir())?;
        shard_rule(&contract, meta.clone()).await?;
        run_command(
            &contract,
            meta.clone(),
            "create queue sq (region bigint, body varchar(100)) \
             partition by global rule r_sq references (region);",
        )
        .await?;
        let queue_id = meta.get_queue("mududb", "sq").await?.unwrap().id();
        let shards = contract.queue_shards(queue_id).await?;
        assert_eq!(shards.len(), 2);

        run_command(
            &contract,
            meta.clone(),
            "insert into sq values (1, 'a'), (200, 'b'), (2, 'c'), (300, 'd');",
        )
        .await?;

        // Per-shard local sequences: each shard allocated its own prefix.
        let runtime0 = contract
            .queue_registry()
            .get((queue_id, shards[0].partition_id))?
            .unwrap();
        let runtime1 = contract
            .queue_registry()
            .get((queue_id, shards[1].partition_id))?
            .unwrap();
        assert_eq!(runtime0.next_seq(), 2);
        assert_eq!(runtime1.next_seq(), 2);

        // The shard bits of every message identify its shard.
        let rows = run_query(&contract, meta.clone(), "receive 10 from sq;").await?;
        assert_eq!(rows.len(), 2, "the round-robin receive drained shard 0");
        let seqs: Vec<u64> = rows.iter().map(row_seq).collect();
        for seq in &seqs {
            assert_eq!(queue_seq_shard(*seq), 0);
        }
        let rows = run_query(&contract, meta.clone(), "receive 10 from sq;").await?;
        assert_eq!(rows.len(), 2, "the next receive drained shard 1");
        let bodies: Vec<String> = rows.iter().map(row_body3).collect();
        for seq in rows.iter().map(row_seq) {
            assert_eq!(queue_seq_shard(seq), 1);
        }
        assert_eq!(bodies, vec!["b", "d"]);

        // ACK targets the shard through the sequence's shard bits.
        let acked =
            run_command(&contract, meta.clone(), &format!("ack sq to {};", seqs[1])).await?;
        assert_eq!(acked, 2);
        let runtime0 = contract
            .queue_registry()
            .get((queue_id, shards[0].partition_id))?
            .unwrap();
        assert_eq!(runtime0.acked_frontier(), 2);
        let runtime1 = contract
            .queue_registry()
            .get((queue_id, shards[1].partition_id))?
            .unwrap();
        assert_eq!(runtime1.acked_frontier(), 0);
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn sharded_queue_rebuilds_each_shard_after_reopen() {
    block_on(async move {
        let meta = Arc::new(TestMetaMgr::new());
        let data_dir = temp_data_dir();
        let contract = make_contract(meta.clone(), data_dir.clone())?;
        shard_rule(&contract, meta.clone()).await?;
        run_command(
            &contract,
            meta.clone(),
            "create queue sq (region bigint, body varchar(100)) \
             partition by global rule r_sq references (region);",
        )
        .await?;
        let queue_id = meta.get_queue("mududb", "sq").await?.unwrap().id();
        run_command(
            &contract,
            meta.clone(),
            "insert into sq values (1, 'a'), (200, 'b');",
        )
        .await?;
        // Claim shard 0's message without acking (the crash redelivers it).
        let rows = run_query(&contract, meta.clone(), "receive 10 from sq;").await?;
        assert_eq!(rows.len(), 1);
        contract.storage().flush_dirty_pages_async().await?;
        drop(contract);

        let contract_b = make_contract(meta.clone(), data_dir)?;
        contract_b.bootstrap_storage_async().await?;
        let shards = contract_b.queue_shards(queue_id).await?;
        // The crashed claim on shard 0 is redelivered.
        let rows = run_query(&contract_b, meta.clone(), "receive 10 from sq;").await?;
        assert_eq!(rows.len(), 1);
        assert_eq!(row_body3(&rows[0]), "a");
        let rows = run_query(&contract_b, meta.clone(), "receive 10 from sq;").await?;
        assert_eq!(rows.len(), 1);
        assert_eq!(row_body3(&rows[0]), "b");
        // The watermarks survive per shard (next local seq keeps moving).
        let runtime0 = contract_b
            .queue_registry()
            .get((queue_id, shards[0].partition_id))?
            .unwrap();
        assert_eq!(runtime0.next_seq(), 1);
        let runtime1 = contract_b
            .queue_registry()
            .get((queue_id, shards[1].partition_id))?
            .unwrap();
        assert_eq!(runtime1.next_seq(), 1);
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn unpartitioned_queue_stays_single_shard() {
    block_on(async move {
        let meta = Arc::new(TestMetaMgr::new());
        let contract = make_contract(meta.clone(), temp_data_dir())?;
        run_command(
            &contract,
            meta.clone(),
            "create queue q (body varchar(100));",
        )
        .await?;
        let queue_id = meta.get_queue("mududb", "q").await?.unwrap().id();
        let shards = contract.queue_shards(queue_id).await?;
        assert_eq!(shards.len(), 1);
        assert_eq!(shards[0].shard_index, 0);
        assert_eq!(shards[0].partition_id, 0);
        run_command(&contract, meta.clone(), "insert into q values ('x');").await?;
        let rows = run_query(&contract, meta.clone(), "receive 1 from q;").await?;
        assert_eq!(row_seq(&rows[0]), 0);
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}
