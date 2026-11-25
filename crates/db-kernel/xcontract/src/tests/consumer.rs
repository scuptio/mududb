#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Named consumer cursor (fan-out) tests over a real `WorkerXContract` and
//! real relation storage: create/receive/ack/GC, cursor persistence across a
//! reopen, and the mutual exclusion with the default competing cursor.

use super::block_on;
use super::queue::{
    make_contract, row_body, row_seq, row_status, run_command, run_query, temp_data_dir,
    wait_for_waiter,
};
use crate::contract::WorkerXContract;
use crate::*;
use ::catalog_testing::test_meta_mgr::TestMetaMgr;
use ::meta::queue_consumer::queue_consumer_schema;

/// Registers the built-in `_queue_consumer` system table in the test catalog,
/// mirroring `MetaMgrImpl::initialize_inner`.
async fn register_consumer_table(meta: &Arc<TestMetaMgr>) -> RS<()> {
    meta.create_table(&queue_consumer_schema()).await
}

async fn make_consumer_contract(
    meta_mgr: Arc<TestMetaMgr>,
    data_dir: String,
) -> RS<Arc<WorkerXContract>> {
    register_consumer_table(&meta_mgr).await?;
    make_contract(meta_mgr, data_dir)
}

#[test]
fn consumer_create_receive_ack_gc_end_to_end() {
    block_on(async move {
        let meta = Arc::new(TestMetaMgr::new());
        let contract = make_consumer_contract(meta.clone(), temp_data_dir()).await?;
        run_command(
            &contract,
            meta.clone(),
            "create queue q (body varchar(100));",
        )
        .await?;
        let queue_id = meta.get_queue("mududb", "q").await?.unwrap().id();
        run_command(
            &contract,
            meta.clone(),
            "insert into q values ('m1'), ('m2'), ('m3'), ('m4');",
        )
        .await?;
        run_command(&contract, meta.clone(), "create consumer cga on q;").await?;
        run_command(&contract, meta.clone(), "create consumer cgb on q;").await?;

        // A fan-out receive claims nothing and does not move the frontier:
        // the same read repeats.
        let rows = run_query(&contract, meta.clone(), "receive 2 from q for cga;").await?;
        let seqs: Vec<u64> = rows.iter().map(row_seq).collect();
        assert_eq!(seqs, vec![0, 1]);
        assert_eq!(
            row_status(&contract, queue_id, 0).await?,
            Some((0, 0)),
            "the fan-out scan left the row ready with its receive count at 0"
        );
        let rows = run_query(&contract, meta.clone(), "receive 2 from q for cga;").await?;
        let seqs: Vec<u64> = rows.iter().map(row_seq).collect();
        assert_eq!(seqs, vec![0, 1], "the frontier did not move on receive");

        // cga acks up to seq 1 (frontier 2); cgb is still at 0, so the GC
        // collects nothing.
        let deleted = run_command(&contract, meta.clone(), "ack q for cga to 1;").await?;
        assert_eq!(deleted, 0);
        assert_eq!(
            contract.consumer_registry().frontier(queue_id, 0, "cga")?,
            Some(2)
        );
        // cgb still sees the whole queue.
        let rows = run_query(&contract, meta.clone(), "receive 10 from q for cgb;").await?;
        let bodies: Vec<String> = rows.iter().map(row_body).collect();
        assert_eq!(bodies, vec!["m1", "m2", "m3", "m4"]);

        // cgb acks up to seq 2 (frontier 3): the GC watermark is min(2, 3),
        // so the prefix both cursors acknowledged (seqs 0 and 1) is
        // physically deleted.
        let deleted = run_command(&contract, meta.clone(), "ack q for cgb to 2;").await?;
        assert_eq!(deleted, 2);
        assert_eq!(row_status(&contract, queue_id, 0).await?, None);
        assert_eq!(row_status(&contract, queue_id, 1).await?, None);
        assert_eq!(
            row_status(&contract, queue_id, 2).await?,
            Some((0, 0)),
            "seq 2 is past cga's frontier and survives"
        );
        // The queue runtime's acked frontier tracks the GC prefix so the
        // depth accounting keeps reflecting live rows.
        assert_eq!(
            contract
                .queue_registry()
                .get((queue_id, 0))?
                .unwrap()
                .acked_frontier(),
            2
        );
        // cga continues from its own frontier.
        let rows = run_query(&contract, meta.clone(), "receive 10 from q for cga;").await?;
        let seqs: Vec<u64> = rows.iter().map(row_seq).collect();
        assert_eq!(seqs, vec![2, 3]);
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn consumer_frontier_survives_reopen() {
    block_on(async move {
        let data_dir = temp_data_dir();
        let meta = Arc::new(TestMetaMgr::new());
        let contract_a = make_consumer_contract(meta.clone(), data_dir.clone()).await?;
        run_command(
            &contract_a,
            meta.clone(),
            "create queue q (body varchar(100));",
        )
        .await?;
        let queue_id = meta.get_queue("mududb", "q").await?.unwrap().id();
        run_command(
            &contract_a,
            meta.clone(),
            "insert into q values ('m1'), ('m2'), ('m3');",
        )
        .await?;
        run_command(&contract_a, meta.clone(), "create consumer cg on q;").await?;
        let deleted = run_command(&contract_a, meta.clone(), "ack q for cg to 0;").await?;
        assert_eq!(deleted, 1);
        contract_a.storage().flush_dirty_pages_async().await?;
        drop(contract_a);

        // Reopen the data directory: the first queue access lazily rebuilds
        // the cursor cache from the committed `_queue_consumer` rows, and the
        // receive continues from the persisted frontier.
        let contract_b = make_consumer_contract(meta.clone(), data_dir).await?;
        contract_b.bootstrap_storage_async().await?;
        let rows = run_query(&contract_b, meta.clone(), "receive 10 from q for cg;").await?;
        let seqs: Vec<u64> = rows.iter().map(row_seq).collect();
        assert_eq!(seqs, vec![1, 2], "the cursor resumed at its frontier");
        assert_eq!(
            contract_b.consumer_registry().frontier(queue_id, 0, "cg")?,
            Some(1)
        );
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn consumer_and_competing_cursor_are_mutually_exclusive() {
    block_on(async move {
        let meta = Arc::new(TestMetaMgr::new());
        let contract = make_consumer_contract(meta.clone(), temp_data_dir()).await?;
        run_command(
            &contract,
            meta.clone(),
            "create queue q (body varchar(100));",
        )
        .await?;
        run_command(&contract, meta.clone(), "insert into q values ('m1');").await?;
        run_command(&contract, meta.clone(), "create consumer cg on q;").await?;

        // The default competing cursor is disabled while the queue has named
        // consumers.
        let err = run_query(&contract, meta.clone(), "receive 1 from q;")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        let err = run_command(&contract, meta.clone(), "ack q to 0;")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);

        // Unknown cursors and duplicate names fail with the object errors.
        let err = run_query(&contract, meta.clone(), "receive 1 from q for missing;")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
        let err = run_command(&contract, meta.clone(), "ack q for missing to 0;")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
        let err = run_command(&contract, meta.clone(), "create consumer cg on q;")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::ObjectAlreadyExists);

        // Dropping the last cursor returns the queue to competing mode.
        run_command(&contract, meta.clone(), "drop consumer cg on q;").await?;
        let err = run_command(&contract, meta.clone(), "drop consumer cg on q;")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
        let rows = run_query(&contract, meta.clone(), "receive 1 from q;").await?;
        assert_eq!(rows.len(), 1);
        assert_eq!(row_seq(&rows[0]), 0);
        assert_eq!(
            row_status(
                &contract,
                meta.get_queue("mududb", "q").await?.unwrap().id(),
                0
            )
            .await?,
            Some((1, 1)),
            "the competing receive claimed the message again"
        );
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn create_consumer_resets_claimed_leftovers() {
    block_on(async move {
        let meta = Arc::new(TestMetaMgr::new());
        let contract = make_consumer_contract(meta.clone(), temp_data_dir()).await?;
        run_command(
            &contract,
            meta.clone(),
            "create queue q (body varchar(100));",
        )
        .await?;
        let queue_id = meta.get_queue("mududb", "q").await?.unwrap().id();
        run_command(
            &contract,
            meta.clone(),
            "insert into q values ('m1'), ('m2');",
        )
        .await?;
        // A competing receive claims seq 0 and commits the claim; its ACK
        // never comes.
        let rows = run_query(&contract, meta.clone(), "receive 1 from q;").await?;
        assert_eq!(row_seq(&rows[0]), 0);
        assert_eq!(row_status(&contract, queue_id, 0).await?, Some((1, 1)));

        // Registering a consumer switches the queue to fan-out mode: the
        // claimed leftover is reset to ready with a zeroed receive count and
        // becomes visible to every cursor.
        run_command(&contract, meta.clone(), "create consumer cg on q;").await?;
        assert_eq!(
            row_status(&contract, queue_id, 0).await?,
            Some((0, 0)),
            "the claimed leftover returned to ready with receive count 0"
        );
        let rows = run_query(&contract, meta.clone(), "receive 10 from q for cg;").await?;
        let seqs: Vec<u64> = rows.iter().map(row_seq).collect();
        assert_eq!(seqs, vec![0, 1]);
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn receive_for_consumer_wait_wakes_on_enqueue() {
    block_on(async move {
        let meta = Arc::new(TestMetaMgr::new());
        let contract = make_consumer_contract(meta.clone(), temp_data_dir()).await?;
        run_command(
            &contract,
            meta.clone(),
            "create queue q (body varchar(100));",
        )
        .await?;
        let queue_id = meta.get_queue("mududb", "q").await?.unwrap().id();
        run_command(&contract, meta.clone(), "create consumer cg on q;").await?;

        // The RECEIVE runs on a spawned task: it finds nothing past the
        // cursor frontier and parks on the queue wait registry.
        let contract_recv = contract.clone();
        let meta_recv = meta.clone();
        let receiver = tokio::task::spawn_local(async move {
            run_query(
                &contract_recv,
                meta_recv,
                "receive 1 from q for cg wait 5000;",
            )
            .await
        });
        wait_for_waiter(&contract, queue_id).await?;
        run_command(&contract, meta.clone(), "insert into q values ('hello');").await?;
        let rows = receiver.await.unwrap()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(row_body(&rows[0]), "hello");
        // The wait wrote nothing: the message stays ready.
        assert_eq!(row_status(&contract, queue_id, 0).await?, Some((0, 0)));
        assert!(contract.queue_wait_registry().waiter_counts()?.is_empty());
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn drop_queue_removes_its_consumer_rows() {
    block_on(async move {
        let meta = Arc::new(TestMetaMgr::new());
        let contract = make_consumer_contract(meta.clone(), temp_data_dir()).await?;
        run_command(
            &contract,
            meta.clone(),
            "create queue q (body varchar(100));",
        )
        .await?;
        let queue_id = meta.get_queue("mududb", "q").await?.unwrap().id();
        run_command(&contract, meta.clone(), "create consumer cga on q;").await?;
        run_command(&contract, meta.clone(), "create consumer cgb on q;").await?;
        assert_eq!(contract.consumer_registry().list(queue_id, 0)?.len(), 2);

        run_command(&contract, meta.clone(), "drop queue q;").await?;
        // The cursor cache and the `_queue_consumer` rows are gone.
        assert!(!contract.consumer_registry().is_loaded(queue_id, 0)?);
        assert!(
            contract
                .storage()
                .consumer_scan_async(queue_id)
                .await?
                .is_empty(),
            "the dropped queue's consumer rows were removed"
        );
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn drop_consumer_wakes_parked_receive_for_wait_forever() {
    block_on(async move {
        let meta = Arc::new(TestMetaMgr::new());
        let contract = make_consumer_contract(meta.clone(), temp_data_dir()).await?;
        run_command(
            &contract,
            meta.clone(),
            "create queue q (body varchar(100));",
        )
        .await?;
        let queue_id = meta.get_queue("mududb", "q").await?.unwrap().id();
        run_command(&contract, meta.clone(), "create consumer cg on q;").await?;

        // The RECEIVE finds nothing past the cursor frontier and parks with
        // no deadline.
        let contract_recv = contract.clone();
        let meta_recv = meta.clone();
        let receiver = tokio::task::spawn_local(async move {
            run_query(
                &contract_recv,
                meta_recv,
                "receive 1 from q for cg wait forever;",
            )
            .await
        });
        wait_for_waiter(&contract, queue_id).await?;
        run_command(&contract, meta.clone(), "drop consumer cg on q;").await?;
        // The drop woke the waiter; its re-scan fails on the missing cursor
        // instead of hanging forever.
        let err = receiver.await.unwrap().unwrap_err();
        assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
        assert!(contract.queue_wait_registry().waiter_counts()?.is_empty());
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn consumer_uncommitted_changes_are_isolated_and_rollback_restores_frontier() {
    block_on(async {
        let meta = Arc::new(TestMetaMgr::new());
        let contract = make_consumer_contract(meta.clone(), temp_data_dir()).await?;
        run_command(
            &contract,
            meta.clone(),
            "create queue q (body varchar(100));",
        )
        .await?;
        let oid = meta.get_queue("mududb", "q").await?.unwrap().id();
        run_command(
            &contract,
            meta.clone(),
            "insert into q values ('a'), ('b');",
        )
        .await?;
        let tx = contract.begin_tx().await?;
        contract.consumer_create(tx.clone(), oid, "cg").await?;
        assert!(contract.consumers_exist_in_tx(tx.clone(), oid).await?);
        assert!(
            !contract.consumers_exist(oid).await?,
            "uncommitted create leaked"
        );
        contract.abort_tx(tx).await?;
        assert!(!contract.consumers_exist(oid).await?);
        run_command(&contract, meta.clone(), "create consumer cg on q;").await?;
        let shard = contract.queue_shards(oid).await?.remove(0);
        let tx = contract.begin_tx().await?;
        contract
            .consumer_ack_and_gc(tx.clone(), &shard, "cg", 0)
            .await?;
        assert_eq!(
            contract
                .consumer_frontier_in_tx(tx.clone(), &shard, "cg")
                .await?,
            Some(1)
        );
        assert_eq!(
            contract.consumer_frontier(&shard, "cg").await?,
            Some(0),
            "uncommitted ACK leaked"
        );
        contract.abort_tx(tx).await?;
        let rows = run_query(&contract, meta.clone(), "receive 10 from q for cg;").await?;
        assert_eq!(rows.iter().map(row_seq).collect::<Vec<_>>(), vec![0, 1]);
        let tx = contract.begin_tx().await?;
        contract.consumer_drop(tx.clone(), oid, "cg").await?;
        assert!(!contract.consumers_exist_in_tx(tx.clone(), oid).await?);
        assert!(
            contract.consumers_exist(oid).await?,
            "uncommitted drop leaked"
        );
        contract.abort_tx(tx).await?;
        assert_eq!(contract.consumer_frontier(&shard, "cg").await?, Some(0));
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}
