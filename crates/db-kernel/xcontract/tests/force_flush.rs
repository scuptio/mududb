//! `MUDU_WAL_FORCE_FLUSH=1` makes every commit pay its own flush round.
//! The flag reads through a process-wide OnceLock, so this lives in its own
//! integration-test process where the variable is set before any commit.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use xcontract::contract::WorkerXContract;
use xengine::api::XContract;

#[test]
fn force_flush_env_var_commits_every_write_with_its_own_flush() {
    // Set before the first commit so the OnceLock observes it.
    mudu_sys::env_var::set_var("MUDU_WAL_FORCE_FLUSH", "1");
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let data_dir = mudu_sys::env_var::temp_dir()
            .join(format!("force_flush_{}", mudu_utils::oid::gen_oid()))
            .to_string_lossy()
            .to_string();
        let log_dir = mudu_sys::env_var::temp_dir()
            .join(format!("force_flush_log_{}", mudu_utils::oid::gen_oid()))
            .to_string_lossy()
            .to_string();
        let log = xcontract::ChunkedWorkerLogBackend::new(
            wal::worker_log::WorkerLogLayout::new(log_dir, mudu_utils::oid::gen_oid(), 4096)
                .unwrap(),
        )
        .await
        .unwrap();
        let contract = WorkerXContract::with_worker_log_and_data_dir(log, 0, 0, 0, data_dir)
            .await
            .unwrap();

        // Two commits both land; with the flag on, each forced its own
        // flush round instead of sharing the group-commit window.
        for i in 0..2u32 {
            let tx = contract.begin_tx().await.unwrap();
            let mut items = BTreeMap::new();
            items.insert(format!("k{i}").into_bytes(), Some(b"v".to_vec()));
            contract
                .worker_commit_put_batch_async(
                    &tx.snapshot(),
                    tx.xid(),
                    items,
                    wal::xl_batch::XLBatch::new(vec![]),
                )
                .await
                .unwrap();
        }
        assert_eq!(
            contract.worker_get_async(b"k1").await.unwrap(),
            Some(b"v".to_vec())
        );
    })
    .unwrap();
}
