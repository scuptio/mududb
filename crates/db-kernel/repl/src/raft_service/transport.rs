use super::{raft_msg_to_frame_bytes, RaftShare};
use ::catalog::repl_cfg::ReplPeerCfg;
use mudu::common::result::RS;
use mudu_raft::core::conf::{ConfNodeValue, ConfVersion};
use mudu_raft::core::message::RaftMsg;
use mudu_raft::RaftHandle;
use mudu_sys::net::AsyncTcpStream;
use mudu_sys::sync::async_::stop_flag::{stop_channel, StopRx};
use mudu_sys::sync::SMutex;
use mudu_sys::tokio;
use mudu_sys::tokio::io::AsyncWriteExt;
use mudu_sys::tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use mudu_utils::notifier::notify_wait;
use mudu_utils::task_async::spawn_local_task;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{trace, warn};

/// Backoff between a failed raft peer connection and the reconnect.
const RAFT_PEER_RECONNECT_BACKOFF: Duration = Duration::from_millis(300);

/// MSFM transport: outbound raft messages are encoded as `Raft*` frames and
/// queued to the per-peer writer tasks. Non-blocking and best-effort, as
/// the raft event loop requires. The address book is dynamic: the conf-sync
/// task inserts/removes peer writers as the committed conf changes.
pub(super) struct MudbRaftTransport {
    pub(super) node_id: u64,
    pub(super) out: SMutex<HashMap<u64, UnboundedSender<Vec<u8>>>>,
}

impl MudbRaftTransport {
    /// Registers a new peer writer's outbound queue (conf-sync).
    fn add_peer(&self, node_id: u64, tx: UnboundedSender<Vec<u8>>) {
        if let Ok(mut out) = self.out.lock() {
            out.insert(node_id, tx);
        }
    }

    /// Drops a removed peer's outbound queue (conf-sync).
    fn remove_peer(&self, node_id: u64) {
        if let Ok(mut out) = self.out.lock() {
            out.remove(&node_id);
        }
    }
}

impl mudu_raft::transport::RaftTransport for MudbRaftTransport {
    fn send(&self, to: u64, msg: RaftMsg) -> RS<()> {
        let msg_type = match &msg {
            RaftMsg::PreVoteReq(_) => "PreVoteReq",
            RaftMsg::PreVoteResp(_) => "PreVoteResp",
            RaftMsg::VoteReq(_) => "VoteReq",
            RaftMsg::VoteResp(_) => "VoteResp",
            RaftMsg::AppendReq(_) => "AppendReq",
            RaftMsg::AppendResp(_) => "AppendResp",
            RaftMsg::ApplyReq(_) => "ApplyReq",
            RaftMsg::ApplyResp(_) => "ApplyResp",
            RaftMsg::UpdateConfReq(_) => "UpdateConfReq",
            RaftMsg::UpdateConfResp(_) => "UpdateConfResp",
        };
        let bytes = raft_msg_to_frame_bytes(self.node_id, &msg)?;
        let tx = self.out.lock().ok().and_then(|out| out.get(&to).cloned());
        if let Some(tx) = tx {
            // a closed channel means the peer writer is gone; raft tolerates
            // the loss and retries on its own schedule
            let sent = tx.send(bytes);
            if sent.is_err() {
                trace!(
                    to,
                    msg_type,
                    "raft transport send dropped (peer writer gone)"
                );
            }
        } else {
            trace!(to, msg_type, "raft transport send dropped (no such peer)");
        }
        Ok(())
    }
}

/// A peer writer task with its own stop channel (conf-sync managed).
struct PeerWriterEntry {
    addr: String,
    stop: mudu_sys::sync::async_::stop_flag::StopTx,
}

/// Drives membership reconciliation: every committed-conf change updates
/// the share's member/voter lists and spawns/stops the per-peer writers.
/// The initial message comes from the watcher's first status delivery and
/// establishes the startup peer set (stored conf wins over the static
/// configuration).
pub(super) async fn run_conf_sync(
    raft: Arc<RaftHandle>,
    share: Arc<RaftShare>,
    transport: Arc<MudbRaftTransport>,
    mut conf_rx: UnboundedReceiver<(ConfNodeValue, ConfNodeValue)>,
    mut stop_rx: StopRx,
) -> RS<()> {
    let mut writers: HashMap<u64, PeerWriterEntry> = HashMap::new();
    loop {
        let pair = tokio::select! {
            msg = conf_rx.recv() => match msg {
                Some(pair) => pair,
                None => return Ok(()),
            },
            _ = stop_rx.changed() => {
                return Ok(());
            }
        };
        let (committed, _new) = pair;
        let (members, voters) = conf_member_ids(&committed);
        trace!(
            node = share.node_id(),
            ?members,
            ?voters,
            "raft conf sync: reconciling membership"
        );
        let peers = resolve_peer_addrs(&members, &share);
        share.update_membership(peers.clone(), voters);
        // Stop writers of removed members (including their address-book
        // entry); start writers for new ones.
        stop_removed_writers(&mut writers, &members, &transport, share.node_id());
        spawn_missing_writers(&mut writers, peers, &transport, share.node_id());
        let _ = &raft;
    }
}

/// The member and voter id lists of the committed conf.
fn conf_member_ids(committed: &ConfNodeValue) -> (Vec<u64>, Vec<u64>) {
    let members: Vec<u64> = committed
        .value
        .node_peer
        .iter()
        .map(|p| p.node_id)
        .collect();
    let voters: Vec<u64> = committed
        .value
        .node_peer
        .iter()
        .filter(|p| p.can_vote)
        .map(|p| p.node_id)
        .collect();
    (members, voters)
}

/// Resolves member addresses: the kernel-side address book (updated by
/// membership changes) first, then the current share entries.
fn resolve_peer_addrs(members: &[u64], share: &RaftShare) -> Vec<ReplPeerCfg> {
    let current = share.peers();
    let mut peers = Vec::with_capacity(members.len());
    for member in members {
        let addr = crate::repl_member_addr(*member).or_else(|| {
            current
                .iter()
                .find(|peer| peer.node_id() == *member)
                .map(|peer| (peer.host().to_string(), peer.port()))
        });
        if let Some((host, port)) = addr {
            peers.push(ReplPeerCfg::new(*member, host, port));
        } else {
            warn!(
                member,
                "conf member has no known replication address; it cannot be reached"
            );
        }
    }
    peers
}

/// Stops the writers of removed members, including their address-book
/// entry.
fn stop_removed_writers(
    writers: &mut HashMap<u64, PeerWriterEntry>,
    members: &[u64],
    transport: &MudbRaftTransport,
    self_node: u64,
) {
    let removed: Vec<u64> = writers
        .keys()
        .filter(|node| !members.contains(node))
        .copied()
        .collect();
    for node in removed {
        if let Some(entry) = writers.remove(&node) {
            entry.stop.stop();
        }
        transport.remove_peer(node);
        if node != self_node {
            let _ = crate::repl_member_addr_unregister(node);
        }
    }
}

/// Starts a writer for every member that lacks one or whose address
/// changed (an address change restarts the writer).
fn spawn_missing_writers(
    writers: &mut HashMap<u64, PeerWriterEntry>,
    peers: Vec<ReplPeerCfg>,
    transport: &MudbRaftTransport,
    self_node: u64,
) {
    for peer in peers {
        if peer.node_id() == self_node {
            continue;
        }
        let addr = format!("{}:{}", peer.host(), peer.port());
        let needs_spawn = writers
            .get(&peer.node_id())
            .is_none_or(|entry| entry.addr != addr);
        if !needs_spawn {
            continue;
        }
        if let Some(entry) = writers.remove(&peer.node_id()) {
            entry.stop.stop();
        }
        let (tx, rx) = unbounded_channel::<Vec<u8>>();
        transport.add_peer(peer.node_id(), tx);
        let (_writer_notifier, writer_waiter) = notify_wait();
        let (writer_stop_tx, writer_stop_rx) = stop_channel();
        trace!(
            node = self_node,
            peer = peer.node_id(),
            %addr,
            "raft conf sync: spawning peer writer"
        );
        let _ = spawn_local_task(
            writer_waiter,
            &format!("raft_peer_writer_{}", peer.node_id()),
            run_peer_writer(addr.clone(), rx, writer_stop_rx),
        );
        writers.insert(
            peer.node_id(),
            PeerWriterEntry {
                addr,
                stop: writer_stop_tx,
            },
        );
    }
}

/// Hooks the conf-change forwarder into the status stream: a small thread
/// watches statuses (they carry conf versions) and forwards the full conf
/// pair (queried from the handle, blocking-safe on that thread) whenever
/// the committed version moves. The first observed status forwards too, so
/// the recovered conf reaches the conf-sync task at startup.
pub(super) fn install_conf_sync_hook(
    raft: &Arc<RaftHandle>,
    share: &Arc<RaftShare>,
    conf_sync_tx: UnboundedSender<(ConfNodeValue, ConfNodeValue)>,
) {
    let raft = raft.clone();
    let share = share.clone();
    if let Err(err) =
        mudu_sys::task::sync::spawn_thread_named("raft-conf-watch".to_string(), move || {
            let rx = raft.watch_status();
            let mut last_committed: Option<ConfVersion> = None;
            loop {
                match rx.recv() {
                    Ok(status) => {
                        let version = status.conf_version_committed;
                        let changed = last_committed.as_ref() != Some(&version);
                        if !changed {
                            continue;
                        }
                        last_committed = Some(version);
                        match raft.conf() {
                            Ok(pair) => {
                                let _ = conf_sync_tx.send(pair);
                            }
                            Err(err) => {
                                warn!("raft conf query failed in conf watcher: {err}");
                            }
                        }
                    }
                    Err(_) => return Ok::<(), mudu::error::MuduError>(()),
                }
            }
        })
    {
        warn!(
            share = share.node_id(),
            "failed to spawn raft conf watcher: {err}"
        );
    }
}

/// Per-peer outbound writer: maintains one connection to the peer's
/// replication endpoint and writes queued raft frames. Reconnects forever
/// (with backoff) until stopped; messages queued during an outage are
/// dropped on reconnect (raft retries on its own schedule).
async fn run_peer_writer(
    addr: String,
    mut out_rx: UnboundedReceiver<Vec<u8>>,
    mut stop_rx: StopRx,
) -> RS<()> {
    while !stop_rx.is_stopped() {
        let connect_result = AsyncTcpStream::connect(addr.as_str()).await;
        let mut stream = match connect_result {
            Ok(stream) => stream,
            Err(err) => {
                trace!(addr = %addr, "raft peer connect failed: {}", err);
                tokio::select! {
                    _ = mudu_sys::task::async_::sleep(RAFT_PEER_RECONNECT_BACKOFF) => {}
                    _ = stop_rx.changed() => {}
                }
                continue;
            }
        };
        if let Err(err) = stream.set_nodelay(true) {
            warn!(addr = %addr, "raft peer set nodelay error: {}", err);
            continue;
        }
        trace!(addr = %addr, "raft peer connection established");
        // Drop frames queued during the outage: they are stale by raft
        // semantics, and the protocol retries heartbeats and appends itself.
        while out_rx.try_recv().is_ok() {}
        loop {
            tokio::select! {
                msg = out_rx.recv() => {
                    let Some(bytes) = msg else {
                        return Ok(());
                    };
                    if let Err(err) = stream.write_all(&bytes).await {
                        warn!(addr = %addr, "raft peer write error, reconnecting: {}", err);
                        break;
                    }
                }
                _ = stop_rx.changed() => {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}
