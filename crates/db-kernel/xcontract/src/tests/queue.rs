#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Queue runtime tests over a real `WorkerXContract` and real relation
//! storage: enqueue synthesis, claim/commit/ack, rollback redelivery, and
//! the lazy runtime rebuild (with crash redelivery) after a reopen.

use super::block_on;
use crate::*;
use ::catalog_testing::test_meta_mgr::TestMetaMgr;
use mudu_sys::env_var::temp_dir;
use mudu_type::data_type::DataType;
use mudu_type::type_family::TypeFamily;
use sql::binder::Binder;
use sql::bound_stmt::BoundStmt;
use sql::plan_ctx::PlanCtx;
use sql::planner::Planner;
use sql_parser::ast::parser::SQLParser;

pub(super) fn make_contract(
    meta_mgr: Arc<TestMetaMgr>,
    data_dir: String,
) -> RS<Arc<WorkerXContract>> {
    make_contract_on_partition(meta_mgr, data_dir, 0)
}

/// Same fixture as [`make_contract`], but the worker's default partition id
/// is `partition_id` — a real server's worker gets a random nonzero OID here
/// (`worker_registry::create_worker_identity`), while queue relations always
/// live on partition 0.
pub(super) fn make_contract_on_partition(
    meta_mgr: Arc<TestMetaMgr>,
    data_dir: String,
    partition_id: OID,
) -> RS<Arc<WorkerXContract>> {
    Ok(Arc::new(WorkerXContract::with_log_and_data_dir(
        WorkerXContractParams {
            meta_mgr,
            log: None,
            log_layout: Default::default(),
            active_sessions: Default::default(),
            worker_id: 0,
            default_unpartitioned_worker_id: 0,
            partition_id,
            data_dir,
            async_runtime: None,
            server_instance_id: 0,
        },
    )?))
}

pub(super) fn temp_data_dir() -> String {
    temp_dir()
        .join(format!("xcontract_queue_{}", gen_oid()))
        .to_string_lossy()
        .to_string()
}

pub(super) async fn run_command(
    contract: &Arc<WorkerXContract>,
    meta: Arc<TestMetaMgr>,
    sql_text: &str,
) -> RS<u64> {
    if meta
        .get_table_by_id(::meta::queue_consumer::QUEUE_CONSUMER_TABLE_ID)
        .await
        .is_err()
    {
        meta.create_table(&::meta::queue_consumer::queue_consumer_schema())
            .await?;
    }
    let stmt = SQLParser::new()?.parse(sql_text)?.into_stmts().remove(0);
    let tx_mgr = contract.begin_tx().await?;
    let bound = Binder::new(meta.clone(), "mududb".to_string())
        .bind_ref(&stmt, &())
        .await?;
    let BoundStmt::Command(command) = bound else {
        return Err(mudu_error!(ErrorCode::Internal, "expected a command"));
    };
    let planner = Planner::new(PlanCtx {
        tx_mgr: tx_mgr.clone(),
        meta_mgr: meta,
        x_contract: contract.clone(),
        async_runtime: None,
    });
    let cmd = planner.plan_command(command).await?;
    let result = async {
        cmd.prepare().await?;
        cmd.run().await?;
        cmd.affected_rows().await
    }
    .await;
    match result {
        Ok(affected) => {
            contract.commit_tx(tx_mgr).await?;
            Ok(affected)
        }
        Err(err) => {
            contract.abort_tx(tx_mgr).await?;
            Err(err)
        }
    }
}

pub(super) async fn run_query(
    contract: &Arc<WorkerXContract>,
    meta: Arc<TestMetaMgr>,
    sql_text: &str,
) -> RS<Vec<TupleRow>> {
    let tx_mgr = contract.begin_tx().await?;
    let result = run_query_in_tx(contract, meta, tx_mgr.clone(), sql_text).await;
    match result {
        Ok(rows) => {
            contract.commit_tx(tx_mgr).await?;
            Ok(rows)
        }
        Err(err) => {
            contract.abort_tx(tx_mgr).await?;
            Err(err)
        }
    }
}

/// Runs a query statement inside the existing transaction `tx_mgr`.
pub(super) async fn run_query_in_tx(
    contract: &Arc<WorkerXContract>,
    meta: Arc<TestMetaMgr>,
    tx_mgr: Arc<dyn TxMgr>,
    sql_text: &str,
) -> RS<Vec<TupleRow>> {
    let stmt = SQLParser::new()?.parse(sql_text)?.into_stmts().remove(0);
    let bound = Binder::new(meta.clone(), "mududb".to_string())
        .bind_ref(&stmt, &())
        .await?;
    let BoundStmt::Query(query) = bound else {
        return Err(mudu_error!(ErrorCode::Internal, "expected a query"));
    };
    let planner = Planner::new(PlanCtx {
        tx_mgr,
        meta_mgr: meta,
        x_contract: contract.clone(),
        async_runtime: None,
    });
    let exec = planner.plan_query(query).await?;
    exec.open().await?;
    let mut rows = Vec::new();
    while let Some(row) = exec.next().await? {
        rows.push(row);
    }
    Ok(rows)
}

pub(super) fn row_seq(row: &TupleRow) -> u64 {
    ::catalog::contract::queue::decode_queue_seq(row.fields()[0].as_ref().unwrap()).unwrap()
}

pub(super) fn row_body(row: &TupleRow) -> String {
    use mudu_type::datum::DatumDyn;
    mudu_contract::tuple::typed_bin::TypedBin::new(
        TypeFamily::String,
        row.fields()[2].as_ref().unwrap().clone(),
    )
    .to_value(&DataType::default_for(TypeFamily::String))
    .unwrap()
    .into_string()
}

/// Reads the `(status, receive_count)` header of one queue row.
pub(super) async fn row_status(
    contract: &Arc<WorkerXContract>,
    table_id: OID,
    seq: u64,
) -> RS<Option<(u8, i32)>> {
    let tx = contract.begin_tx().await?;
    let row = contract
        .read_key(
            tx.clone(),
            table_id,
            &VecDatum::new(vec![(0, ::catalog::contract::queue::queue_seq_datum(seq)?)]),
            &VecSelTerm::new(vec![2, 3]),
            &OptRead::default(),
        )
        .await?;
    contract.abort_tx(tx).await?;
    let Some(fields) = row else {
        return Ok(None);
    };
    let status = ::catalog::contract::queue::decode_queue_status(fields[0].as_ref().unwrap())?;
    let receive_count =
        ::catalog::contract::queue::decode_queue_receive_count(fields[1].as_ref().unwrap())?;
    Ok(Some((status, receive_count)))
}

#[test]
fn queue_enqueue_receive_ack_end_to_end() {
    block_on(async move {
        let meta = Arc::new(TestMetaMgr::new());
        let contract = make_contract(meta.clone(), temp_data_dir())?;
        run_command(
            &contract,
            meta.clone(),
            "create queue q (body varchar(100));",
        )
        .await?;
        let queue = meta.get_queue("mududb", "q").await?.unwrap();
        let queue_id = queue.id();
        // CREATE QUEUE derived the dead-letter queue.
        let dlq = meta.get_queue("mududb", "q_dlq").await?.unwrap();

        run_command(
            &contract,
            meta.clone(),
            "insert into q values ('m1'), ('m2');",
        )
        .await?;
        assert_eq!(
            contract
                .queue_registry()
                .get((queue_id, 0))?
                .unwrap()
                .next_seq(),
            2
        );

        let rows = run_query(&contract, meta.clone(), "receive 1 from q;").await?;
        assert_eq!(rows.len(), 1);
        assert_eq!(row_seq(&rows[0]), 0);
        assert_eq!(row_body(&rows[0]), "m1");
        assert_eq!(
            row_status(&contract, queue_id, 0).await?,
            Some((1, 1)),
            "the claimed row carries status=claimed and receive_count=1"
        );

        let acked = run_command(&contract, meta.clone(), "ack q to 0;").await?;
        assert_eq!(acked, 1);
        assert_eq!(
            contract
                .queue_registry()
                .get((queue_id, 0))?
                .unwrap()
                .acked_frontier(),
            1
        );
        assert_eq!(row_status(&contract, queue_id, 0).await?, None);

        let rows = run_query(&contract, meta.clone(), "receive from q;").await?;
        assert_eq!(rows.len(), 1);
        assert_eq!(row_seq(&rows[0]), 1);
        assert_eq!(row_body(&rows[0]), "m2");
        // The DLQ stayed empty (registered at create, never allocated).
        let dlq_runtime = contract.queue_registry().get((dlq.id(), 0))?.unwrap();
        assert_eq!(dlq_runtime.next_seq(), 0);
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn receive_rollback_returns_message_to_ready() {
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
        run_command(&contract, meta.clone(), "insert into q values ('m1');").await?;

        // Claim in a transaction, then roll it back: the staged claim is
        // discarded, so the message is ready again and its receive count is
        // unchanged.
        {
            let stmt = SQLParser::new()?
                .parse("receive 1 from q;")?
                .into_stmts()
                .remove(0);
            let tx_mgr = contract.begin_tx().await?;
            let bound = Binder::new(meta.clone(), "mududb".to_string())
                .bind_ref(&stmt, &())
                .await?;
            let BoundStmt::Query(query) = bound else {
                panic!("expected a query");
            };
            let planner = Planner::new(PlanCtx {
                tx_mgr: tx_mgr.clone(),
                meta_mgr: meta.clone(),
                x_contract: contract.clone(),
                async_runtime: None,
            });
            let exec = planner.plan_query(query).await?;
            exec.open().await?;
            assert!(exec.next().await?.is_some());
            contract.abort_tx(tx_mgr).await?;
        }
        assert_eq!(
            row_status(&contract, queue_id, 0).await?,
            Some((0, 0)),
            "after rollback the row is ready with its receive count intact"
        );

        let rows = run_query(&contract, meta.clone(), "receive 1 from q;").await?;
        assert_eq!(rows.len(), 1);
        assert_eq!(row_seq(&rows[0]), 0);
        assert_eq!(row_status(&contract, queue_id, 0).await?, Some((1, 1)));
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn reopen_rebuilds_counters_and_redelivers_crashed_claims() {
    block_on(async move {
        let data_dir = temp_data_dir();
        let meta = Arc::new(TestMetaMgr::new());
        let contract_a = make_contract(meta.clone(), data_dir.clone())?;
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
            "insert into q values ('m1'), ('m2');",
        )
        .await?;
        // Claim seq 0 and commit the claim; the consumer then "crashes"
        // before the ACK.
        let rows = run_query(&contract_a, meta.clone(), "receive 1 from q;").await?;
        assert_eq!(row_seq(&rows[0]), 0);
        contract_a.storage().flush_dirty_pages_async().await?;
        drop(contract_a);

        // Reopen the data directory: the first queue access lazily rebuilds
        // the runtime counters from the relation and resets the crashed
        // claim to ready (receive count preserved).
        let contract_b = make_contract(meta.clone(), data_dir)?;
        contract_b.bootstrap_storage_async().await?;
        let rows = run_query(&contract_b, meta.clone(), "receive 10 from q;").await?;
        let seqs: Vec<u64> = rows.iter().map(row_seq).collect();
        assert_eq!(seqs, vec![0, 1], "the crashed claim is redelivered");
        let runtime = contract_b.queue_registry().get((queue_id, 0))?.unwrap();
        assert_eq!(runtime.next_seq(), 2);
        assert_eq!(runtime.acked_frontier(), 0);
        assert_eq!(
            row_status(&contract_b, queue_id, 0).await?,
            Some((1, 2)),
            "the redelivery kept the pre-crash receive count and added one"
        );
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

/// Regression test for the restart rebuild scanning the wrong partition: a
/// real server's worker default partition id is a random nonzero OID, while
/// a queue relation always lives on partition 0
/// (`DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID`). The reopen here runs with a
/// nonzero default partition, so a rebuild keyed on the worker's default
/// partition would miss the queue relation entirely: the counters would
/// restart at zero (the next enqueue then collides with the surviving rows)
/// and the crashed claim would never be reset for redelivery.
#[test]
fn reopen_with_nonzero_default_partition_rebuilds_queue_runtime() {
    block_on(async move {
        let data_dir = temp_data_dir();
        let meta = Arc::new(TestMetaMgr::new());
        let partition_id = 7;
        let contract_a = make_contract_on_partition(meta.clone(), data_dir.clone(), partition_id)?;
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
            "insert into q values ('m1'), ('m2');",
        )
        .await?;
        let rows = run_query(&contract_a, meta.clone(), "receive 1 from q;").await?;
        assert_eq!(row_seq(&rows[0]), 0);
        contract_a.storage().flush_dirty_pages_async().await?;
        drop(contract_a);

        let contract_b = make_contract_on_partition(meta.clone(), data_dir, partition_id)?;
        contract_b.bootstrap_storage_async().await?;
        let rows = run_query(&contract_b, meta.clone(), "receive 10 from q;").await?;
        let seqs: Vec<u64> = rows.iter().map(row_seq).collect();
        assert_eq!(seqs, vec![0, 1], "the crashed claim is redelivered");
        let runtime = contract_b.queue_registry().get((queue_id, 0))?.unwrap();
        assert_eq!(runtime.next_seq(), 2);
        assert_eq!(runtime.acked_frontier(), 0);
        assert_eq!(
            row_status(&contract_b, queue_id, 0).await?,
            Some((1, 2)),
            "the redelivery kept the pre-crash receive count and added one"
        );
        // The rebuilt watermark keeps the next enqueue clear of the live
        // rows (a zeroed counter would reissue seq 0 and hit "existing key").
        run_command(&contract_b, meta.clone(), "insert into q values ('m3');").await?;
        let runtime = contract_b.queue_registry().get((queue_id, 0))?.unwrap();
        assert_eq!(runtime.next_seq(), 3);
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn two_receives_in_one_transaction_do_not_double_deliver() {
    block_on(async move {
        let meta = Arc::new(TestMetaMgr::new());
        let contract = make_contract(meta.clone(), temp_data_dir())?;
        run_command(
            &contract,
            meta.clone(),
            "create queue q (body varchar(100));",
        )
        .await?;
        run_command(
            &contract,
            meta.clone(),
            "insert into q values ('m1'), ('m2');",
        )
        .await?;

        // The second RECEIVE in the same transaction observes the first
        // one's staged claims (read-your-writes) and moves past them.
        let tx_mgr = contract.begin_tx().await?;
        let first =
            run_query_in_tx(&contract, meta.clone(), tx_mgr.clone(), "receive 1 from q;").await?;
        let second =
            run_query_in_tx(&contract, meta.clone(), tx_mgr.clone(), "receive 1 from q;").await?;
        contract.commit_tx(tx_mgr).await?;
        assert_eq!(row_seq(&first[0]), 0);
        assert_eq!(row_seq(&second[0]), 1);
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

/// Waits until a `RECEIVE ... WAIT` waiter is registered on the queue.
pub(super) async fn wait_for_waiter(contract: &Arc<WorkerXContract>, queue_id: OID) -> RS<()> {
    for _ in 0..2000 {
        let parked = contract
            .queue_wait_registry()
            .waiter_counts()?
            .get(&(queue_id, 0))
            .copied()
            .unwrap_or(0)
            > 0;
        if parked {
            return Ok(());
        }
        mudu_sys::task::async_::sleep(Duration::from_millis(1)).await?;
    }
    Err(mudu_error!(
        ErrorCode::InvalidState,
        "the receive did not park on the queue wait registry"
    ))
}

#[test]
fn receive_wait_wakes_on_enqueue_commit() {
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

        // The RECEIVE runs on a spawned task: it finds the queue empty and
        // parks on the wait registry.
        let contract_recv = contract.clone();
        let meta_recv = meta.clone();
        let receiver = tokio::task::spawn_local(async move {
            run_query(&contract_recv, meta_recv, "receive 1 from q wait 5000;").await
        });
        wait_for_waiter(&contract, queue_id).await?;
        run_command(&contract, meta.clone(), "insert into q values ('hello');").await?;
        let rows = receiver.await.unwrap()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(row_body(&rows[0]), "hello");
        // The registration was cleaned up when the statement finished.
        assert!(contract.queue_wait_registry().waiter_counts()?.is_empty());
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn receive_wait_forever_wakes_on_enqueue() {
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

        let contract_recv = contract.clone();
        let meta_recv = meta.clone();
        let receiver = tokio::task::spawn_local(async move {
            run_query(&contract_recv, meta_recv, "receive 1 from q wait forever;").await
        });
        wait_for_waiter(&contract, queue_id).await?;
        run_command(&contract, meta.clone(), "insert into q values ('m1');").await?;
        let rows = receiver.await.unwrap()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(row_seq(&rows[0]), 0);
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn receive_wait_timeout_returns_empty_batch() {
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

        let started = mudu_sys::time::instant_now();
        let rows = run_query(&contract, meta.clone(), "receive 3 from q wait 40;").await?;
        assert!(rows.is_empty());
        assert!(
            started.elapsed() >= Duration::from_millis(30),
            "the wait parked until its timeout"
        );
        // The timed-out waiter unregistered itself.
        assert!(contract.queue_wait_registry().waiter_counts()?.is_empty());
        assert_eq!(
            contract
                .queue_registry()
                .get((queue_id, 0))?
                .unwrap()
                .next_seq(),
            0
        );
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn receive_wait_timeout_returns_partial_batch() {
    block_on(async move {
        let meta = Arc::new(TestMetaMgr::new());
        let contract = make_contract(meta.clone(), temp_data_dir())?;
        run_command(
            &contract,
            meta.clone(),
            "create queue q (body varchar(100));",
        )
        .await?;
        run_command(&contract, meta.clone(), "insert into q values ('m1');").await?;

        // Two of the three requested messages never arrive: the wait expires
        // and returns the one claimed message.
        let rows = run_query(&contract, meta.clone(), "receive 3 from q wait 40;").await?;
        assert_eq!(rows.len(), 1);
        assert_eq!(row_body(&rows[0]), "m1");
        assert_eq!(
            row_status(
                &contract,
                meta.get_queue("mududb", "q").await?.unwrap().id(),
                0
            )
            .await?,
            Some((1, 1)),
            "the partial-batch claim committed before the wait"
        );
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn receive_wait_forever_returns_existing_partial_batch() {
    block_on(async {
        let meta = Arc::new(TestMetaMgr::new());
        let contract = make_contract(meta.clone(), temp_data_dir())?;
        run_command(
            &contract,
            meta.clone(),
            "create queue q (body varchar(100));",
        )
        .await?;
        run_command(&contract, meta.clone(), "insert into q values ('m1');").await?;
        let rows = mudu_sys::task::async_::timeout(
            Duration::from_secs(1),
            run_query(&contract, meta.clone(), "receive 3 from q wait forever;"),
        )
        .await
        .unwrap()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(row_body(&rows[0]), "m1");
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn drop_queue_wakes_parked_wait_forever() {
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

        let contract_recv = contract.clone();
        let meta_recv = meta.clone();
        let receiver = tokio::task::spawn_local(async move {
            run_query(&contract_recv, meta_recv, "receive 1 from q wait forever;").await
        });
        wait_for_waiter(&contract, queue_id).await?;
        run_command(&contract, meta.clone(), "drop queue q;").await?;
        // The dropped queue woke the waiter; its rescan fails instead of
        // hanging forever.
        let err = receiver.await.unwrap().unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        assert!(contract.queue_wait_registry().waiter_counts()?.is_empty());
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

/// Executes a command without committing the caller's transaction.
pub(super) async fn run_command_in_tx(
    contract: &Arc<WorkerXContract>,
    meta: Arc<TestMetaMgr>,
    tx: Arc<dyn TxMgr>,
    text: &str,
) -> RS<()> {
    let stmt = SQLParser::new()?.parse(text)?.into_stmts().remove(0);
    let BoundStmt::Command(command) = Binder::new(meta.clone(), "mududb".to_string())
        .bind_ref(&stmt, &())
        .await?
    else {
        panic!("expected command");
    };
    let planner = Planner::new(PlanCtx {
        tx_mgr: tx,
        meta_mgr: meta,
        x_contract: contract.clone(),
        async_runtime: None,
    });
    let command = planner.plan_command(command).await?;
    command.prepare().await?;
    command.run().await
}

#[test]
fn queue_sequences_follow_enqueue_commit_order_not_transaction_start() {
    block_on(async {
        let meta = Arc::new(TestMetaMgr::new());
        let contract = make_contract(meta.clone(), temp_data_dir())?;
        run_command(
            &contract,
            meta.clone(),
            "create queue q (body varchar(100));",
        )
        .await?;
        let earlier = contract.begin_tx().await?;
        run_command(
            &contract,
            meta.clone(),
            "insert into q values ('committed-first');",
        )
        .await?;
        run_command_in_tx(
            &contract,
            meta.clone(),
            earlier.clone(),
            "insert into q values ('committed-second');",
        )
        .await?;
        contract.commit_tx(earlier).await?;
        let rows = run_query(&contract, meta, "receive 10 from q;").await?;
        assert_eq!(rows.iter().map(row_seq).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(
            rows.iter().map(row_body).collect::<Vec<_>>(),
            vec!["committed-first", "committed-second"]
        );
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn drop_queue_removes_partition_zero_on_nonzero_worker() {
    block_on(async {
        let meta = Arc::new(TestMetaMgr::new());
        let contract = make_contract_on_partition(meta.clone(), temp_data_dir(), 123)?;
        run_command(
            &contract,
            meta.clone(),
            "create queue q (body varchar(100));",
        )
        .await?;
        let oid = meta.get_queue("mududb", "q").await?.unwrap().id();
        run_command(&contract, meta.clone(), "insert into q values ('drop-me');").await?;
        assert!(contract
            .storage()
            .get_relation_async(oid, Some(0))
            .await
            .is_ok());
        run_command(&contract, meta, "drop queue q;").await?;
        assert!(contract
            .storage()
            .get_relation_async(oid, Some(0))
            .await
            .is_err());
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}
