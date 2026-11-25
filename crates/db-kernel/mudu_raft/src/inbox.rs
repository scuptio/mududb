//! Bounded synchronous ingress with asynchronous reception and priority capacity.

use std::io::Write;
use std::mem::size_of;
use std::sync::Arc;

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::tokio;
use mudu_sys::tokio::sync::{mpsc, watch, OwnedSemaphorePermit, Semaphore};

use crate::api::{NodeStatus, ProposeResult, UpdateConfError, UpdateConfResult};
use crate::completion::CompletionSender;
use crate::core::conf::{ConfNodeValue, ConfValue, ConfVersion, NID};
use crate::core::message::{RaftMsg, TermIndex};
use crate::entry::ReplEntry;

const NORMAL_MESSAGES: usize = 1024;
const CONTROL_MESSAGES: usize = 128;
const NORMAL_BYTES: usize = 64 * 1024 * 1024;
const CONTROL_BYTES: usize = 1024 * 1024;

struct Envelope {
    event: RaftIn,
    _bytes: Option<OwnedSemaphorePermit>,
}

#[derive(Clone)]
struct Lane {
    tx: mpsc::Sender<Envelope>,
    bytes: Arc<Semaphore>,
}

/// Input message of a raft node's event loop.
pub enum RaftIn {
    /// Read a page of locally durable committed entries for recovery.
    ReadCommitted {
        /// Last entry already replayed by the caller.
        after: u64,
        /// Maximum entry count; capped at 256.
        limit: usize,
        /// Recovery page completion.
        resp: CompletionSender<crate::replay::CommittedBatch>,
    },
    /// Subscribe to committed entries without a blocking receiver.
    WatchCommitsAsync {
        /// Ordered committed stream; queue saturation stops the driver safely.
        tx: tokio::sync::mpsc::Sender<(u64, ReplEntry)>,
    },
    /// Subscribe to the latest durable status without a blocking receiver.
    WatchStatusAsync {
        /// Coalesced latest status.
        tx: tokio::sync::watch::Sender<NodeStatus>,
    },
    /// An inbound raft message from a peer.
    Net {
        /// Sender node id.
        from: NID,
        /// The raft message.
        msg: RaftMsg,
    },
    /// A local proposal request.
    Propose {
        /// Entry to replicate.
        entry: ReplEntry,
        /// Membership pair used to prove payload durability, if applicable.
        expected_conf: Option<crate::stream::MembershipVersion>,
        /// Leader term captured at guarded proposal enqueue.
        expected_term: Option<u64>,
        /// Revocable data/session certificates checked at protocol admission.
        guards: Vec<crate::admission::AdmissionGuard>,
        /// Completion channel of the waiter.
        resp: CompletionSender<ProposeResult>,
    },
    /// Install an externally produced snapshot frontier.
    InstallSnapshot {
        /// The snapshot frontier `(term, index)`; the log is
        /// left-truncated at `index`.
        frontier: TermIndex,
        /// Completed after the frontier is durable.
        resp: CompletionSender<RS<()>>,
    },
    /// Force log compaction up to the current commit index.
    ForceCompact {
        /// Completed after the compaction is durable.
        resp: CompletionSender<RS<()>>,
    },
    /// Begin a joint-consensus re-configuration (leader only).
    UpdateConf {
        /// Membership value of the new configuration.
        conf_value: ConfValue,
        /// Voting nodes of the new configuration (subset of `nid_log`).
        nid_vote: Vec<NID>,
        /// Log-replicating nodes of the new configuration.
        nid_log: Vec<NID>,
        /// External stream proof, validated at serialized protocol admission.
        fence: Option<crate::admission::MembershipFence>,
        /// Acknowledged (accepted + durable, or the rejection reason)
        /// after the new configuration is persisted.
        ack: CompletionSender<Result<ConfVersion, UpdateConfError>>,
        /// Completed when the re-configuration commits (or fails when
        /// leadership is lost or the node stops).
        done: CompletionSender<UpdateConfResult>,
    },
    /// Read the current committed and new configuration.
    QueryConf {
        /// Receives `(committed, new)` immediately.
        resp: CompletionSender<(ConfNodeValue, ConfNodeValue)>,
    },
    /// An injected tick (drives time when `manual_ticks` is set).
    Tick,
    /// Stop the event loop (persists everything first).
    Stop,
}

/// Cloneable, nonblocking ingress handle of a Raft driver.
/// Queue limits cover both encoded payload bytes and message count.
#[derive(Clone)]

pub struct RaftSender {
    normal: Lane,
    control: Lane,
    stop: watch::Sender<bool>,
}

/// Asynchronous reception half of the raft ingress channel.
pub struct RaftInbox {
    normal: mpsc::Receiver<Envelope>,
    control: mpsc::Receiver<Envelope>,
    stop: watch::Receiver<bool>,
    control_streak: usize,
}

fn lane(messages: usize, bytes: usize) -> (Lane, mpsc::Receiver<Envelope>) {
    let (tx, rx) = mpsc::channel(messages);
    (
        Lane {
            tx,
            bytes: Arc::new(Semaphore::new(bytes)),
        },
        rx,
    )
}

/// Creates a matched raft ingress channel.
pub fn inbox() -> (RaftSender, RaftInbox) {
    let (normal, normal_rx) = lane(NORMAL_MESSAGES, NORMAL_BYTES);
    let (control, control_rx) = lane(CONTROL_MESSAGES, CONTROL_BYTES);
    let (stop, stop_rx) = watch::channel(false);
    (
        RaftSender {
            normal,
            control,
            stop,
        },
        RaftInbox {
            normal: normal_rx,
            control: control_rx,
            stop: stop_rx,
            control_streak: 0,
        },
    )
}

impl RaftSender {
    /// Enqueues immediately, returning ResourceBusy on capacity exhaustion.
    /// Stop has a separate signal and remains deliverable when both lanes are full.
    pub fn send(&self, mut event: RaftIn) -> RS<()> {
        if matches!(event, RaftIn::Stop) {
            self.stop.send_replace(true);
            return Ok(());
        }
        if *self.stop.borrow() {
            return Err(mudu_error!(
                ErrorCode::ChannelClosed,
                "raft driver is stopping"
            ));
        }
        let normal = matches!(&event, RaftIn::Propose { .. })
            || matches!(&event, RaftIn::Net { msg: RaftMsg::AppendReq(req), .. } if !req.entries.is_empty());
        let lane = if normal { &self.normal } else { &self.control };
        let bytes = event_bytes(&event)?;
        let permits = u32::try_from(bytes)
            .map_err(|_| mudu_error!(ErrorCode::ResourceBusy, "raft event exceeds byte budget"))?;
        let guard = lane
            .bytes
            .clone()
            .try_acquire_many_owned(permits)
            .map_err(|_| {
                mudu_error!(ErrorCode::ResourceBusy, "raft inbox byte budget exhausted")
            })?;
        let bytes = if let RaftIn::Propose { resp, .. } = &mut event {
            // Keep the payload budget until commit/rejection, rather than
            // releasing it when an uncommitted proposal leaves the inbox.
            resp.reserve(guard);
            None
        } else {
            Some(guard)
        };
        lane.tx
            .try_send(Envelope {
                event,
                _bytes: bytes,
            })
            .map_err(|e| match e {
                mpsc::error::TrySendError::Full(_) => {
                    mudu_error!(ErrorCode::ResourceBusy, "raft inbox is full")
                }
                mpsc::error::TrySendError::Closed(_) => {
                    mudu_error!(ErrorCode::ChannelClosed, "raft inbox is closed")
                }
            })
    }
}

impl RaftInbox {
    /// Awaits the next inbound event, or `None` once closed.
    pub async fn recv(&mut self) -> Option<RaftIn> {
        if *self.stop.borrow() {
            return None;
        }
        // Bound priority bursts so normal traffic also makes progress.
        if self.control_streak >= 32 {
            if let Ok(event) = self.normal.try_recv() {
                self.control_streak = 0;
                return Some(event.event);
            }
        }
        tokio::select! {
            biased;
            _ = self.stop.changed() => None,
            event = self.control.recv() => {
                self.control_streak += 1;
                event.map(|e| e.event)
            }
            event = self.normal.recv() => {
                self.control_streak = 0;
                event.map(|e| e.event)
            }
        }
    }

    /// Closes the reception half.
    pub fn close(&mut self) {
        self.normal.close();
        self.control.close();
        // Dropping queued requests closes their completion channels.
        while self.normal.try_recv().is_ok() {}
        while self.control.try_recv().is_ok() {}
    }
}

#[derive(Default)]
struct ByteCount(usize);

impl Write for ByteCount {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len());
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn event_bytes(event: &RaftIn) -> RS<usize> {
    let guard_bytes = match event {
        RaftIn::Propose { guards, .. } => guards
            .len()
            .saturating_mul(size_of::<crate::admission::AdmissionGuard>()),
        RaftIn::UpdateConf {
            fence: Some(fence), ..
        } => fence
            .guards
            .len()
            .saturating_mul(size_of::<crate::admission::AdmissionGuard>()),
        _ => 0,
    };
    let mut count = ByteCount(size_of::<Envelope>().saturating_add(guard_bytes));
    let result = match event {
        RaftIn::Propose { entry, .. } => rmp_serde::encode::write(&mut count, entry),
        RaftIn::Net { msg, .. } => rmp_serde::encode::write(&mut count, msg),
        RaftIn::UpdateConf {
            conf_value,
            nid_vote,
            nid_log,
            ..
        } => rmp_serde::encode::write(&mut count, &(conf_value, nid_vote, nid_log)),
        _ => return Ok(count.0),
    };
    result.map_err(|e| mudu_error!(ErrorCode::Encode, "measure raft event error", e))?;
    Ok(count.0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::completion::completion;
    use crate::entry::ReplEntry;

    fn proposal() -> RaftIn {
        let (resp, _wait) = completion();
        RaftIn::Propose {
            entry: ReplEntry::Barrier { term: 1 },
            expected_conf: None,
            expected_term: None,
            guards: Vec::new(),
            resp,
        }
    }

    #[test]
    fn proposal_flood_leaves_control_capacity_and_stop_never_blocks() {
        let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
        runtime.block_on(async {
            let (tx, mut rx) = inbox();
            for _ in 0..NORMAL_MESSAGES {
                tx.send(proposal()).unwrap();
            }
            assert_eq!(
                tx.send(proposal()).unwrap_err().ec(),
                ErrorCode::ResourceBusy
            );
            for _ in 0..CONTROL_MESSAGES {
                tx.send(RaftIn::Tick).unwrap();
            }
            assert_eq!(
                tx.send(RaftIn::Tick).unwrap_err().ec(),
                ErrorCode::ResourceBusy
            );
            assert!(matches!(rx.recv().await, Some(RaftIn::Tick)));
            tx.send(RaftIn::Stop).unwrap();
            assert!(rx.recv().await.is_none());
            assert_eq!(
                tx.send(proposal()).unwrap_err().ec(),
                ErrorCode::ChannelClosed
            );
            rx.close();
            assert_eq!(tx.normal.bytes.available_permits(), NORMAL_BYTES);
        });
    }

    #[test]
    fn pending_proposal_keeps_its_byte_reservation() {
        let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
        runtime.block_on(async {
            let (tx, mut rx) = inbox();
            tx.send(proposal()).unwrap();
            let reserved = tx.normal.bytes.available_permits();
            assert!(reserved < NORMAL_BYTES);
            let event = rx.recv().await.unwrap();
            assert_eq!(tx.normal.bytes.available_permits(), reserved);
            drop(event);
            assert_eq!(tx.normal.bytes.available_permits(), NORMAL_BYTES);
        });
    }

    #[test]
    fn oversized_payload_cannot_bypass_byte_budget() {
        let (tx, _rx) = inbox();
        let (resp, _wait) = completion();
        let event = RaftIn::Propose {
            entry: crate::entry::ReplEntry::CommitVec {
                term: 1,
                // Every record serializes to ≥40 bytes, so this is strictly
                // over the byte budget.
                per_worker: vec![(0, 0, [0u8; 32]); NORMAL_BYTES / 32],
            },
            expected_conf: None,
            expected_term: None,
            guards: Vec::new(),
            resp,
        };
        assert_eq!(tx.send(event).unwrap_err().ec(), ErrorCode::ResourceBusy);
        assert_eq!(tx.normal.bytes.available_permits(), NORMAL_BYTES);
    }

    #[test]
    fn priority_bursts_do_not_starve_proposals() {
        let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
        runtime.block_on(async {
            let (tx, mut rx) = inbox();
            tx.send(proposal()).unwrap();
            for _ in 0..CONTROL_MESSAGES {
                tx.send(RaftIn::Tick).unwrap();
            }
            for _ in 0..32 {
                assert!(matches!(rx.recv().await, Some(RaftIn::Tick)));
            }
            assert!(matches!(rx.recv().await, Some(RaftIn::Propose { .. })));
        });
    }
}
