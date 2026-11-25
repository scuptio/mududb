//! The raft service's I/O thread: peer-writer reconciliation on committed
//! conf changes, and the status watcher mirroring the durable status into
//! the shared view. The commit/apply dispatch and the leader-elect heal live
//! in the data-plane service now (`crate::data_plane`).

use super::transport::{install_conf_sync_hook, run_conf_sync, MudbRaftTransport};
use super::RaftShare;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::core::conf::ConfNodeValue;
use mudu_raft::RaftHandle;
use mudu_sys::sync::async_::stop_flag::StopRx;
use mudu_sys::task::sync::{spawn_thread_named, SJoinHandle};
use mudu_sys::tokio::sync::mpsc::unbounded_channel;
use mudu_utils::notifier::notify_wait;
use mudu_utils::task_async::{spawn_local_task, CurrentThreadTaskRuntime};
use std::sync::Arc;

pub(super) fn spawn_raft_io_thread(
    share: Arc<RaftShare>,
    raft: Arc<RaftHandle>,
    transport: Arc<MudbRaftTransport>,
    stop_rx: StopRx,
) -> RS<SJoinHandle<RS<()>>> {
    spawn_thread_named("raft-io".to_string(), move || {
        let runtime = CurrentThreadTaskRuntime::new()
            .map_err(|e| mudu_error!(ErrorCode::Tokio, "build raft io runtime error", e))?;
        runtime.block_on(async move {
            // Conf sync: the status watcher thread observes committed conf
            // changes and forwards the conf pair here; this task reconciles
            // the peer writers, the share's member/voter lists and the
            // kernel-side address book with the committed conf.
            let (conf_sync_tx, conf_sync_rx) =
                unbounded_channel::<(ConfNodeValue, ConfNodeValue)>();
            install_conf_sync_hook(&raft, conf_sync_tx)?;
            let (_sync_notifier, sync_waiter) = notify_wait();
            let _ = spawn_local_task(
                sync_waiter,
                "raft_conf_sync",
                run_conf_sync(
                    raft.clone(),
                    share.clone(),
                    transport,
                    conf_sync_rx,
                    stop_rx.clone(),
                ),
            );
            let (_status_stop, status_wait) = notify_wait();
            let status_share = share.clone();
            let status_raft = raft.clone();
            spawn_local_task(status_wait, "raft_status_watch", async move {
                run_status_watcher(status_raft, status_share.clone()).await
            })?;
            let mut stop = stop_rx.clone();
            stop.changed().await;
            Ok(())
        })
    })
}

/// Mirrors durable status into the share. The data plane's convergence
/// watcher owns the write gate (`serving`); this watcher only publishes the
/// durable status and defensively closes the gate on shutdown.
async fn run_status_watcher(raft: Arc<RaftHandle>, share: Arc<RaftShare>) -> RS<()> {
    let mut rx = raft.watch_status_async()?;
    loop {
        let status = rx.borrow_and_update().clone();
        share.update_status(&status);
        if rx.changed().await.is_err() {
            break;
        }
    }
    share.set_serving(false);
    Ok(())
}
