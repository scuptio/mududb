use super::transport::{install_conf_sync_hook, run_conf_sync, MudbRaftTransport};
use super::RaftShare;
use crate::heal::run_heal_driver;
use crate::ApplierMsg;
use ::catalog::contract::message_bus_api::ServerInstanceId;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::core::conf::ConfNodeValue;
use mudu_raft::{RaftHandle, RaftRole, ReplEntry};
use mudu_sys::sync::async_::stop_flag::StopRx;
use mudu_sys::task::sync::{spawn_thread_named, SJoinHandle};
use mudu_sys::tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use mudu_utils::notifier::notify_wait;
use mudu_utils::task_async::{spawn_local_task, CurrentThreadTaskRuntime};
use std::sync::Arc;
use tracing::trace;

pub(super) fn spawn_raft_io_thread(
    server_instance_id: ServerInstanceId,
    share: Arc<RaftShare>,
    raft: Arc<RaftHandle>,
    transport: Arc<MudbRaftTransport>,
    heal_rx: UnboundedReceiver<u64>,
    stop_rx: StopRx,
) -> RS<SJoinHandle<RS<()>>> {
    spawn_thread_named("raft-io".to_string(), move || {
        let runtime = CurrentThreadTaskRuntime::new()
            .map_err(|e| mudu_error!(ErrorCode::Tokio, "build raft io runtime error", e))?;
        runtime.block_on(async move {
            let (_heal_notifier, heal_waiter) = notify_wait();
            let _ = spawn_local_task(
                heal_waiter,
                "raft_heal_driver",
                run_heal_driver(
                    server_instance_id,
                    share.clone(),
                    raft.clone(),
                    heal_rx,
                    stop_rx.clone(),
                ),
            );
            // Conf sync: the status watcher thread observes committed conf
            // changes and forwards the conf pair here; this task reconciles
            // the peer writers, the share's member/voter lists and the
            // kernel-side address book with the committed conf.
            let (conf_sync_tx, conf_sync_rx) =
                unbounded_channel::<(ConfNodeValue, ConfNodeValue)>();
            install_conf_sync_hook(&raft, &share, conf_sync_tx);
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
            let mut stop = stop_rx.clone();
            stop.changed().await;
            Ok(())
        })
    })
}

/// Blocking status watcher: mirrors raft status into the share, closes the
/// write gate on leadership loss and triggers leader-elect healing on
/// transitions to leader. Exits when the raft group stops (watch channel
/// closes).
pub(super) fn spawn_status_watcher(
    raft: Arc<RaftHandle>,
    share: Arc<RaftShare>,
) -> RS<SJoinHandle<RS<()>>> {
    spawn_thread_named("raft-status-watch".to_string(), move || {
        let rx = raft.watch_status();
        let mut was_leader = false;
        loop {
            match rx.recv() {
                Ok(status) => {
                    trace!(
                        role = ?status.role,
                        term = status.term,
                        leader = ?status.leader_id,
                        "raft status observed"
                    );
                    let is_leader = status.role == RaftRole::Leader;
                    share.update_status(&status);
                    if is_leader && !was_leader {
                        share.set_serving(false);
                        share.trigger_heal();
                    } else if !is_leader {
                        share.set_serving(false);
                    }
                    was_leader = is_leader;
                }
                Err(_) => return Ok(()),
            }
        }
    })
}

/// Blocking commit dispatcher: forwards every committed `CommitVec` point to
/// the node's committed vector and to the owning worker's applier. Exits
/// when the raft group stops.
pub(super) fn spawn_commit_dispatch(
    server_instance_id: ServerInstanceId,
    raft: Arc<RaftHandle>,
    share: Arc<RaftShare>,
) -> RS<SJoinHandle<RS<()>>> {
    spawn_thread_named("raft-commit-dispatch".to_string(), move || {
        let rx = raft.watch_committed();
        loop {
            match rx.recv() {
                Ok((index, entry)) => {
                    share.record_delivered_index(index);
                    if let ReplEntry::CommitVec { term, per_worker } = entry {
                        for (worker_index, lsn, hash) in per_worker {
                            share.record_committed(worker_index, lsn, hash);
                            let tx = crate::repl_worker(server_instance_id, worker_index)
                                .ok()
                                .and_then(|handle| handle.state().applier_tx().ok().flatten());
                            if let Some(tx) = tx {
                                let _ = tx.send(ApplierMsg::Promote {
                                    lsn,
                                    hash,
                                    raft_term: term,
                                    raft_index: index,
                                    respond: None,
                                });
                            }
                        }
                    }
                }
                Err(_) => return Ok(()),
            }
        }
    })
}
