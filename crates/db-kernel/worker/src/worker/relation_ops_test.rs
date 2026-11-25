#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

//! Tests for `WorkerRuntime`'s direct relation ops (XContract table access)
//! and the session-bound `WorkerLocal` surface.

use super::worker_test::{sql_execute, test_registry, test_worker};
use super::*;
use ::conn::worker_local::{WorkerExecute, WorkerLocal};
use async_trait::async_trait;
use std::sync::Arc;
// ---------------------------------------------------------------------------
// relation_ops: direct XContract table access (relation get/update/insert)
// ---------------------------------------------------------------------------

fn i32_datum(v: i32) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}

#[test]
#[cfg_attr(miri, ignore)]
fn relation_insert_and_get_roundtrip_autocommit() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;
        worker.initialize().await.unwrap();
        sql_execute(&worker, 0, "CREATE TABLE rt (id INT PRIMARY KEY, v INT);")
            .await
            .unwrap();

        let session_id = worker.create_session(1).unwrap();
        worker
            .relation_insert_in_session(
                session_id,
                None,
                "rt",
                vec![(0, i32_datum(1))],
                vec![(1, i32_datum(100))],
            )
            .await
            .unwrap();

        let row = worker
            .relation_get_in_session(session_id, None, "rt", vec![(0, i32_datum(1))], vec![0, 1])
            .await
            .unwrap()
            .expect("row must exist");
        assert_eq!(row[1], Some(i32_datum(100)));

        // A missing key resolves to None.
        let missing = worker
            .relation_get_in_session(session_id, None, "rt", vec![(0, i32_datum(2))], vec![0, 1])
            .await
            .unwrap();
        assert!(missing.is_none());

        // A duplicate primary key fails with ObjectAlreadyExists.
        let err = worker
            .relation_insert_in_session(
                session_id,
                None,
                "rt",
                vec![(0, i32_datum(1))],
                vec![(1, i32_datum(200))],
            )
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::ObjectAlreadyExists);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn relation_update_applies_values_and_deltas() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;
        worker.initialize().await.unwrap();
        sql_execute(&worker, 0, "CREATE TABLE ru (id INT PRIMARY KEY, v INT);")
            .await
            .unwrap();
        let session_id = worker.create_session(1).unwrap();
        worker
            .relation_insert_in_session(
                session_id,
                None,
                "ru",
                vec![(0, i32_datum(7))],
                vec![(1, i32_datum(100))],
            )
            .await
            .unwrap();

        // Absolute assignment.
        let affected = worker
            .relation_update_in_session(
                session_id,
                None,
                "ru",
                vec![(0, i32_datum(7))],
                vec![(1, i32_datum(50))],
                vec![],
            )
            .await
            .unwrap();
        assert_eq!(affected, 1);
        let row = worker
            .relation_get_in_session(session_id, None, "ru", vec![(0, i32_datum(7))], vec![1])
            .await
            .unwrap()
            .expect("row must exist");
        assert_eq!(row[0], Some(i32_datum(50)));

        // Delta assignment evaluated on the locked row.
        let affected = worker
            .relation_update_in_session(
                session_id,
                None,
                "ru",
                vec![(0, i32_datum(7))],
                vec![],
                vec![(1, ::xengine::api::DeltaOp::Add, i32_datum(5))],
            )
            .await
            .unwrap();
        assert_eq!(affected, 1);
        let row = worker
            .relation_get_in_session(session_id, None, "ru", vec![(0, i32_datum(7))], vec![1])
            .await
            .unwrap()
            .expect("row must exist");
        assert_eq!(row[0], Some(i32_datum(55)));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn relation_ops_run_inside_session_transaction() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;
        worker.initialize().await.unwrap();
        sql_execute(&worker, 0, "CREATE TABLE rtx (id INT PRIMARY KEY, v INT);")
            .await
            .unwrap();

        let session_a = worker.create_session(1).unwrap();
        let session_b = worker.create_session(2).unwrap();
        worker
            .execute_tx_async(session_a, WorkerExecute::BeginTx)
            .await
            .unwrap();
        worker
            .relation_insert_in_session(
                session_a,
                None,
                "rtx",
                vec![(0, i32_datum(1))],
                vec![(1, i32_datum(10))],
            )
            .await
            .unwrap();

        // The owning session reads its staged write...
        let row = worker
            .relation_get_in_session(session_a, None, "rtx", vec![(0, i32_datum(1))], vec![1])
            .await
            .unwrap();
        assert_eq!(row.expect("staged row")[0], Some(i32_datum(10)));
        // ...while another session does not see it before commit.
        let row = worker
            .relation_get_in_session(session_b, None, "rtx", vec![(0, i32_datum(1))], vec![1])
            .await
            .unwrap();
        assert!(row.is_none());

        worker
            .execute_tx_async(session_a, WorkerExecute::CommitTx)
            .await
            .unwrap();
        let row = worker
            .relation_get_in_session(session_b, None, "rtx", vec![(0, i32_datum(1))], vec![1])
            .await
            .unwrap();
        assert_eq!(row.expect("committed row")[0], Some(i32_datum(10)));

        // The abort path discards a staged insert.
        worker
            .execute_tx_async(session_a, WorkerExecute::BeginTx)
            .await
            .unwrap();
        worker
            .relation_insert_in_session(
                session_a,
                None,
                "rtx",
                vec![(0, i32_datum(2))],
                vec![(1, i32_datum(20))],
            )
            .await
            .unwrap();
        worker
            .execute_tx_async(session_a, WorkerExecute::RollbackTx)
            .await
            .unwrap();
        let row = worker
            .relation_get_in_session(session_a, None, "rtx", vec![(0, i32_datum(2))], vec![1])
            .await
            .unwrap();
        assert!(row.is_none());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn relation_ops_reject_unknown_tables_and_queues() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;
        worker.initialize().await.unwrap();
        sql_execute(&worker, 0, "CREATE QUEUE rq (body varchar(100));")
            .await
            .unwrap();
        let session_id = worker.create_session(1).unwrap();

        let err = worker
            .relation_get_in_session(
                session_id,
                None,
                "no_such",
                vec![(0, i32_datum(1))],
                vec![0],
            )
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::ObjectNotFound);

        let err = worker
            .relation_insert_in_session(
                session_id,
                None,
                "rq",
                vec![(0, i32_datum(1))],
                vec![(1, b"body".to_vec())],
            )
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);

        let err = worker
            .relation_update_in_session(
                session_id,
                None,
                "rq",
                vec![(0, i32_datum(1))],
                vec![(1, b"body".to_vec())],
                vec![],
            )
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);

        // A bare name resolves through the application schema search path:
        // an unknown app schema falls back to `mududb` and finds the queue
        // (a queue read still rejects, but with the queue-specific error).
        let err = worker
            .relation_get_in_session(
                session_id,
                Some("no_such_app"),
                "rq",
                vec![(0, i32_datum(1))],
                vec![0],
            )
            .await
            .unwrap();
        assert!(err.is_none());
    })
    .unwrap()
}

// ---------------------------------------------------------------------------
// session_bound: WorkerLocal context surface of a session-bound runtime
// ---------------------------------------------------------------------------

struct NullBus;

#[async_trait]
impl mudu_utils::channel::ChannelEndpoint for NullBus {
    fn local_endpoint(&self) -> OID {
        0
    }
    async fn send(&self, _dst: OID, _message: mudu::channel::OutgoingMessage) -> RS<u64> {
        unimplemented!()
    }
    async fn recv(
        &self,
        _filter: mudu::channel::RecvFilter,
        _deadline_ms: Option<u64>,
    ) -> RS<mudu::channel::Envelope> {
        unimplemented!()
    }
    fn register_kind_handler(
        &self,
        _kind: mudu::channel::MessageKind,
        _handler: mudu_utils::channel::KindHandler,
    ) -> RS<()> {
        unimplemented!()
    }
    fn unregister_kind_handler(&self, _kind: mudu::channel::MessageKind) -> RS<()> {
        unimplemented!()
    }
}

#[test]
#[cfg_attr(miri, ignore)]
fn session_bound_runtime_exposes_context_accessors() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;
        worker.initialize().await.unwrap();
        sql_execute(&worker, 0, "CREATE TABLE sb (id INT PRIMARY KEY, v INT);")
            .await
            .unwrap();

        let session_id = worker.create_session(1).unwrap();
        let local = new_session_bound_worker_runtime(worker.clone(), session_id);
        let local_ref: &dyn WorkerLocal = local.as_ref();

        // Plain accessors forward to the worker.
        let _ = local_ref.x_contract();
        let _ = local_ref.meta_mgr();
        assert!(local_ref.fs_service().is_ok());
        assert_eq!(local_ref.current_session_id(), Some(session_id));

        // open_argv on the local worker (0 or self) mints a session; a
        // remote worker id is rejected.
        let opened = local_ref.open_argv_async(0).await.unwrap();
        assert_ne!(opened, 0);
        let opened2 = local_ref.open_argv_async(worker.worker_id()).await.unwrap();
        assert_ne!(opened2, 0);
        let err = local_ref.open_argv_async(999_999).await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);

        // Relation forwards reach the worker's relation ops.
        local_ref
            .relation_insert(
                session_id,
                None,
                "sb",
                vec![(0, i32_datum(1))],
                vec![(1, i32_datum(50))],
            )
            .await
            .unwrap();
        let row = local_ref
            .relation_get(session_id, None, "sb", vec![(0, i32_datum(1))], vec![1])
            .await
            .unwrap();
        assert_eq!(row.expect("row")[0], Some(i32_datum(50)));
        let affected = local_ref
            .relation_update(
                session_id,
                None,
                "sb",
                vec![(0, i32_datum(1))],
                vec![(1, i32_datum(51))],
                vec![],
            )
            .await
            .unwrap();
        assert_eq!(affected, 1);

        // worker_channel() reads the registered channel (registered here via a
        // null mock), and panics without one — covered elsewhere.
        mudu_utils::channel::register_channel(
            worker.server_instance_id(),
            worker.worker_id(),
            &(Arc::new(NullBus) as mudu_utils::channel::ChannelEndpointRef),
        )
        .unwrap();
        let _ = local_ref.worker_channel();
        mudu_utils::channel::unregister_channel(worker.server_instance_id(), worker.worker_id())
            .unwrap();
    })
    .unwrap()
}
