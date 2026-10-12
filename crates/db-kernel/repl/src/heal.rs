//! Leader-elect heal: a newly elected leader converges every worker to the
//! last raft-committed vector before client writes are ungated.
//!
//! Per worker, the heal promotes the chain to the committed vector point
//! (the applier promotes from local staging; frames missing locally are
//! fetched from voters via `ReplHealFetch`), then discards the speculative
//! staged tail, and finally proposes `Barrier{term}`. Only when the Barrier
//! commits does the write gate open (`RaftShare::set_serving(true)`). The
//! healed WAL end equals the committed point, so the new leader resumes
//! production without LSN reuse (design invariant: committed points can
//! never conflict).

use crate::raft_service::RaftShare;
use crate::{clear_ack_watermarks, read_next_frame, repl_worker, ApplierMsg};
use ::catalog::contract::message_bus_api::ServerInstanceId;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::{
    decode_error_response, decode_repl_heal_response_frame, encode_repl_heal_fetch_request,
    ErrorResponse, MessageType, ReplHealFetchReq,
};
use mudu_raft::core::role::RaftRole;
use mudu_raft::{RaftHandle, ReplEntry};
use mudu_sys::net::AsyncTcpStream;
use mudu_sys::sync::async_::stop_flag::{stop_channel, StopRx};
use mudu_sys::tokio;
use mudu_sys::tokio::io::AsyncWriteExt;
use mudu_sys::tokio::sync::mpsc::UnboundedReceiver;
use mudu_sys::tokio::sync::oneshot;
use std::sync::Arc;
use std::time::Duration;
use tracing::{trace, warn};

/// Backoff between heal rounds while the committed vector or a worker has
/// not converged yet.
const HEAL_RETRY_BACKOFF: Duration = Duration::from_millis(100);

/// Poll interval while waiting for a worker's chain to cover its committed
/// point during a heal round.
const HEAL_WORKER_POLL: Duration = Duration::from_millis(50);

/// Budget of one applier request/response round-trip (discard-staging).
const HEAL_WORKER_TIMEOUT: Duration = Duration::from_secs(10);

/// Overall budget of one heal round over every worker.
const HEAL_ROUND_TIMEOUT: Duration = Duration::from_secs(15);

/// Budget for the leader-elect Barrier to commit; on expiry the heal
/// retries the whole round while the node is still leader.
const HEAL_BARRIER_TIMEOUT: Duration = Duration::from_secs(10);

/// Budget for one heal-fetch round-trip to a voter.
const HEAL_FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Drives leader-election heals: each trigger (a transition to leader) runs
/// the heal to completion or aborts it on leadership loss / stop.
pub(crate) async fn run_heal_driver(
    server_instance_id: ServerInstanceId,
    share: Arc<RaftShare>,
    raft: Arc<RaftHandle>,
    mut trigger_rx: UnboundedReceiver<u64>,
    mut stop_rx: StopRx,
) -> RS<()> {
    loop {
        tokio::select! {
            trigger = trigger_rx.recv() => {
                let Some(term) = trigger else {
                    return Ok(());
                };
                trace!(term, "leader-elect heal triggered");
                if let Err(err) =
                    heal_until_serving(server_instance_id, &share, &raft, stop_rx.clone()).await
                {
                    warn!("leader-elect heal failed: {}", err);
                }
            }
            _ = stop_rx.changed() => {
                return Ok(());
            }
        }
    }
}

/// Runs heal rounds until the Barrier commits (writes ungated), the node
/// loses leadership, or the driver is stopped.
async fn heal_until_serving(
    server_instance_id: ServerInstanceId,
    share: &Arc<RaftShare>,
    raft: &Arc<RaftHandle>,
    mut stop_rx: StopRx,
) -> RS<()> {
    share.set_serving(false);
    // Ack watermarks from a previous leadership epoch are meaningless:
    // after the healed WAL truncation, new segments reuse LSNs, and a stale
    // `stored_lsn >= seg_hi` from the old epoch would open the majority
    // gate without the follower having the new content.
    clear_ack_watermarks(server_instance_id)?;
    loop {
        if stop_rx.is_stopped() || share.role() != RaftRole::Leader {
            return Ok(());
        }
        // Seal the previous leadership first: the Barrier's commit drives
        // the raft commit index to a known point, and every earlier
        // committed entry is then durable on a quorum by definition.
        let waiter = raft.propose(ReplEntry::Barrier { term: share.term() })?;
        // The propose waiter is a blocking sync receiver: waiting on it
        // inline would stall this thread's peer writers (and with them
        // the raft heartbeats the Barrier needs to commit). Bridge the
        // wait onto a short-lived blocking thread instead.
        let (barrier_tx, barrier_rx) = oneshot::channel();
        mudu_sys::task::sync::spawn_thread_named("heal-barrier-wait".to_string(), move || {
            let _ = barrier_tx.send(waiter.recv());
            Ok::<_, mudu::error::MuduError>(())
        })?;
        let barrier_index = match mudu_sys::timeout(HEAL_BARRIER_TIMEOUT, barrier_rx).await {
            Some(Ok(Ok((index, _term)))) => index,
            Some(Ok(Err(err))) => {
                trace!("leader-elect barrier failed (leadership lost): {}", err);
                return Ok(());
            }
            Some(Err(_closed)) => {
                return Ok(());
            }
            None => {
                warn!("leader-elect barrier timed out; will retry while still leader");
                tokio::select! {
                    _ = mudu_sys::task::async_::sleep(HEAL_RETRY_BACKOFF) => {}
                    _ = stop_rx.changed() => {
                        return Ok(());
                    }
                }
                continue;
            }
        };
        // The committed vector is only authoritative once the local commit
        // dispatcher has delivered every entry up to the Barrier's index;
        // otherwise the heal would promote to a stale vector and the leader
        // would resume production below the true committed point (an LSN
        // collision).
        if !wait_delivered_index(share, barrier_index, &mut stop_rx).await {
            return Ok(());
        }
        // Promote every worker to the (now authoritative) committed vector,
        // drop the speculative staged tails, then ungate.
        let converged = heal_workers_once(server_instance_id, share).await?;
        if converged {
            discard_staging(server_instance_id, share).await?;
            share.set_serving(true);
            trace!("leader-elect heal complete, client writes ungated");
            return Ok(());
        }
        tokio::select! {
            _ = mudu_sys::task::async_::sleep(HEAL_RETRY_BACKOFF) => {}
            _ = stop_rx.changed() => {
                return Ok(());
            }
        }
    }
}

/// Waits until the local commit dispatcher has delivered every raft entry
/// up to `index`; `false` on stop or leadership loss.
async fn wait_delivered_index(share: &Arc<RaftShare>, index: u64, stop_rx: &mut StopRx) -> bool {
    loop {
        if stop_rx.is_stopped() || share.role() != RaftRole::Leader {
            return false;
        }
        if share.delivered_index() >= index {
            return true;
        }
        tokio::select! {
            _ = mudu_sys::task::async_::sleep(Duration::from_millis(10)) => {}
            _ = stop_rx.changed() => {
                return false;
            }
        }
    }
}

/// One heal round over every worker in the committed vector: waits for the
/// worker's chain to cover each committed point (the commit dispatcher
/// delivers every `CommitVec` to the applier in raft order, which is also
/// chain order), heal-fetching missing frames from voters when the local
/// staging does not cover the range. Returns `true` when every worker
/// reached the vector.
async fn heal_workers_once(
    server_instance_id: ServerInstanceId,
    share: &Arc<RaftShare>,
) -> RS<bool> {
    let vector = share.committed_vector();
    let deadline = mudu_sys::time::instant_now() + HEAL_ROUND_TIMEOUT;
    for (worker_index, (lsn, _hash)) in &vector {
        let worker_index = *worker_index;
        let lsn = *lsn;
        let handle = match repl_worker(server_instance_id, worker_index) {
            Ok(handle) => handle,
            Err(_) => return Ok(false),
        };
        loop {
            let cell = handle.state().chain()?;
            // The cell is a resume point (next LSN); the vector carries
            // segment end LSNs, so converged means `resume > seg_hi`.
            if cell.0 > lsn {
                break;
            }
            if mudu_sys::time::instant_now() >= deadline {
                warn!(
                    worker_index,
                    "leader-elect heal: worker chain at resume {} did not reach committed lsn {} in time",
                    cell.0, lsn
                );
                return Ok(false);
            }
            // Give the applier a few staging/dispatch cycles to advance on
            // its own; otherwise pull the missing committed range from any
            // voter and inject it.
            tokio::select! {
                _ = mudu_sys::task::async_::sleep(HEAL_WORKER_POLL) => {}
            }
            let still = handle.state().chain()?;
            if still.0 == cell.0 {
                let frames = heal_fetch_from_voters(share, worker_index, cell.0, lsn + 1).await?;
                if let Some(tx) = handle.state().applier_tx()? {
                    tx.send(ApplierMsg::HealFrames { frames }).map_err(|_| {
                        mudu_error!(
                            ErrorCode::ChannelClosed,
                            format!("promote applier of worker {worker_index} is gone")
                        )
                    })?;
                }
            }
        }
    }
    Ok(true)
}

/// Tells every worker's applier to drop the staged frames beyond its
/// committed point (the speculative tail of the previous leadership).
async fn discard_staging(server_instance_id: ServerInstanceId, share: &Arc<RaftShare>) -> RS<()> {
    let vector = share.committed_vector();
    for (worker_index, _) in vector {
        let Ok(handle) = repl_worker(server_instance_id, worker_index) else {
            continue;
        };
        let Some(tx) = handle.state().applier_tx()? else {
            continue;
        };
        let (respond_tx, respond_rx) = oneshot::channel();
        tx.send(ApplierMsg::DiscardStaging {
            respond: Some(respond_tx),
        })
        .map_err(|_| {
            mudu_error!(
                ErrorCode::ChannelClosed,
                format!("promote applier of worker {worker_index} is gone")
            )
        })?;
        match mudu_sys::timeout(HEAL_WORKER_TIMEOUT, respond_rx).await {
            Some(Ok(result)) => {
                result?;
            }
            Some(Err(_closed)) => {}
            None => {
                warn!(worker_index, "leader-elect discard-staging timed out");
            }
        }
    }
    Ok(())
}

/// Fetches the committed frames `[lsn_lo, lsn_hi)` of one worker from the
/// first voter that answers (its repl listener serves heal-fetch from its
/// WAL and staging).
async fn heal_fetch_from_voters(
    share: &Arc<RaftShare>,
    worker_index: u32,
    lsn_lo: u64,
    lsn_hi: u64,
) -> RS<Vec<Vec<u8>>> {
    let mut last_err = None;
    for peer in share.peers() {
        if peer.node_id() == share.node_id() {
            continue;
        }
        let addr = format!("{}:{}", peer.host(), peer.port());
        match heal_fetch_from_peer(&addr, worker_index, lsn_lo, lsn_hi).await {
            Ok(frames) => return Ok(frames),
            Err(err) => {
                trace!(
                    worker_index,
                    addr = %addr,
                    "heal fetch from voter failed: {}", err
                );
                last_err = Some(err);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        mudu_error!(
            ErrorCode::Network,
            format!("no voter reachable for heal fetch of worker {worker_index}")
        )
    }))
}

async fn heal_fetch_from_peer(
    addr: &str,
    worker_index: u32,
    lsn_lo: u64,
    lsn_hi: u64,
) -> RS<Vec<Vec<u8>>> {
    let mut stream = AsyncTcpStream::connect(addr).await.map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            format!("connect heal-fetch voter {} error", addr),
            e
        )
    })?;
    stream
        .set_nodelay(true)
        .map_err(|e| mudu_error!(ErrorCode::Network, "set heal-fetch nodelay error", e))?;
    let request =
        encode_repl_heal_fetch_request(1, &ReplHealFetchReq::new(worker_index, lsn_lo, lsn_hi))?;
    stream
        .write_all(&request)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, "write heal-fetch request error", e))?;
    let (_stop_tx, mut stop_rx) = stop_channel();
    let mut read_buf: Vec<u8> = Vec::with_capacity(8192);
    let mut chunk = vec![0u8; 8192];
    let read_result = mudu_sys::timeout(HEAL_FETCH_TIMEOUT, async {
        loop {
            let Some(frame) =
                read_next_frame(&mut stream, &mut read_buf, &mut chunk, &mut stop_rx).await?
            else {
                return Err(mudu_error!(
                    ErrorCode::Network,
                    "heal-fetch voter closed the connection"
                ));
            };
            match frame.header().message_type() {
                MessageType::ReplHealResp => {
                    let response = decode_repl_heal_response_frame(&frame)?;
                    if response.worker_id() == worker_index {
                        return Ok(response.into_frames());
                    }
                }
                MessageType::Error => {
                    let error = decode_error_response(&frame)?;
                    return Err(repl_remote_error(&error));
                }
                _ => {}
            }
        }
    })
    .await;
    match read_result {
        Some(result) => result,
        None => Err(mudu_error!(
            ErrorCode::TimedOut,
            format!("heal fetch of worker {worker_index} from {addr} timed out")
        )),
    }
}

fn repl_remote_error(error: &ErrorResponse) -> mudu::error::MuduError {
    let ec = ErrorCode::from_u32(error.code()).unwrap_or(ErrorCode::Internal);
    mudu_error!(
        ec,
        format!("heal-fetch voter returned error: {}", error.message())
    )
}
