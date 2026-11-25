//! Bounded peer queues with reserved control capacity and exact wire charging.
use super::{ClusterFrame, CLUSTER_FRAME_MAX_BYTES};
use catalog::contract::channel_kinds::{CLUSTER_CONTROL_RAFT, CLUSTER_EPOCH_TELL};
use mudu::channel::{DeliveryMode, Envelope};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde::Serialize;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};

const DATA_ITEMS: usize = 1024;
const DATA_BYTES: usize = 64 * 1024 * 1024 + 4;
const CONTROL_ITEMS: usize = 128;
const CONTROL_BYTES: usize = 8 * 1024 * 1024;
const CONTROL_FRAME_BYTES: usize = 256 * 1024;
const CONTROL_BURST: usize = 32;

pub(super) struct QueuedFrame {
    pub(super) bytes: Vec<u8>,
    pub(super) deadline: Option<u64>,
    // Retained after dequeue through the actual socket write or discard.
    _charge: OwnedSemaphorePermit,
}
#[derive(Clone)]
struct Lane {
    tx: mpsc::Sender<QueuedFrame>,
    bytes: Arc<Semaphore>,
}
#[derive(Clone)]
pub(super) struct PeerQueue {
    control: Lane,
    data: Lane,
}
pub(super) struct PeerReceiver {
    control: mpsc::Receiver<QueuedFrame>,
    data: mpsc::Receiver<QueuedFrame>,
    burst: usize,
}

pub(super) fn peer_queue() -> (PeerQueue, PeerReceiver) {
    queue_with_limits(DATA_ITEMS, DATA_BYTES, CONTROL_ITEMS, CONTROL_BYTES)
}
fn queue_with_limits(
    data_items: usize,
    data_bytes: usize,
    control_items: usize,
    control_bytes: usize,
) -> (PeerQueue, PeerReceiver) {
    let (data_tx, data) = mpsc::channel(data_items);
    let (control_tx, control) = mpsc::channel(control_items);
    (
        PeerQueue {
            control: Lane {
                tx: control_tx,
                bytes: Arc::new(Semaphore::new(control_bytes)),
            },
            data: Lane {
                tx: data_tx,
                bytes: Arc::new(Semaphore::new(data_bytes)),
            },
        },
        PeerReceiver {
            control,
            data,
            burst: 0,
        },
    )
}
impl PeerQueue {
    pub(super) fn send(&self, envelope: Envelope) -> RS<()> {
        let control = envelope.delivery() == DeliveryMode::Response
            || matches!(envelope.kind(), CLUSTER_CONTROL_RAFT | CLUSTER_EPOCH_TELL);
        let deadline = envelope.deadline_ms();
        let frame = ClusterFrame::Envelope(envelope);
        // Count before allocation; the caller never allocates an uncharged
        // encoded copy while another producer consumes the remaining budget.
        let mut counter = ByteCount(0);
        frame
            .serialize(&mut rmp_serde::Serializer::new(&mut counter))
            .map_err(|e| mudu_error!(ErrorCode::Encode, "measure cluster frame", e))?;
        let length = counter.0;
        if length > CLUSTER_FRAME_MAX_BYTES as usize {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "cluster frame exceeds wire limit"
            ));
        }
        let lane = if control && length <= CONTROL_FRAME_BYTES {
            &self.control
        } else {
            &self.data
        };
        let charge = lane
            .bytes
            .clone()
            .try_acquire_many_owned((length + 4) as u32)
            .map_err(|_| {
                mudu_error!(
                    ErrorCode::ResourceBusy,
                    "cluster peer byte budget exhausted"
                )
            })?;
        let slot = lane.tx.try_reserve().map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => mudu_error!(
                ErrorCode::ResourceBusy,
                "cluster peer item budget exhausted"
            ),
            mpsc::error::TrySendError::Closed(_) => {
                mudu_error!(ErrorCode::ChannelClosed, "cluster peer queue closed")
            }
        })?;
        let bytes = rmp_serde::to_vec(&frame)
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode cluster frame", e))?;
        if bytes.len() != length {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "cluster frame size changed during encoding"
            ));
        }
        slot.send(QueuedFrame {
            bytes,
            deadline,
            _charge: charge,
        });
        Ok(())
    }
}
impl PeerReceiver {
    pub(super) async fn recv(&mut self) -> Option<QueuedFrame> {
        if self.burst >= CONTROL_BURST {
            if let Ok(frame) = self.data.try_recv() {
                self.burst = 0;
                return Some(frame);
            }
        }
        if let Ok(frame) = self.control.try_recv() {
            self.burst = self.burst.saturating_add(1);
            return Some(frame);
        }
        if let Ok(frame) = self.data.try_recv() {
            self.burst = 0;
            return Some(frame);
        }
        // A closed lane can still have buffered items, already drained above.
        tokio::select! {
            biased;
            frame = self.control.recv(), if !self.control.is_closed() => {
                self.burst = self.burst.saturating_add(1);
                if frame.is_some() { frame } else { self.data.recv().await }
            }
            frame = self.data.recv(), if !self.data.is_closed() => {
                self.burst = 0;
                if frame.is_some() { frame } else { self.control.recv().await }
            }
            else => None,
        }
    }
}
struct ByteCount(usize);
impl Write for ByteCount {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("frame length overflow"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use mudu::channel::MessageKind;
    fn envelope(control: bool) -> Envelope {
        Envelope::new(
            1,
            None,
            1,
            2,
            if control {
                CLUSTER_CONTROL_RAFT
            } else {
                MessageKind::User(1)
            },
            vec![0xff; 10],
            DeliveryMode::FireAndForget,
        )
    }
    #[tokio::test]
    async fn saturated_data_keeps_control_capacity_and_bounds_bursts() {
        let (queue, mut rx) = queue_with_limits(1, 65536, 64, 65536);
        queue.send(envelope(false)).unwrap();
        assert_eq!(
            queue.send(envelope(false)).unwrap_err().ec(),
            ErrorCode::ResourceBusy
        );
        for _ in 0..33 {
            queue.send(envelope(true)).unwrap();
        }
        for _ in 0..CONTROL_BURST {
            let frame: ClusterFrame =
                rmp_serde::from_slice(&rx.recv().await.unwrap().bytes).unwrap();
            assert!(matches!(frame, ClusterFrame::Envelope(e) if e.kind() == CLUSTER_CONTROL_RAFT));
        }
        let frame: ClusterFrame = rmp_serde::from_slice(&rx.recv().await.unwrap().bytes).unwrap();
        assert!(matches!(frame, ClusterFrame::Envelope(e) if e.kind() == MessageKind::User(1)));
    }
    #[tokio::test]
    async fn dequeued_frames_keep_exact_byte_charge_until_write_completes() {
        let encoded = rmp_serde::to_vec(&ClusterFrame::Envelope(envelope(false))).unwrap();
        let bytes = encoded.len() + 4;
        let (queue, mut rx) = queue_with_limits(8, bytes, 1, 1024);
        queue.send(envelope(false)).unwrap();
        let held = rx.recv().await.unwrap();
        assert_eq!(
            queue.send(envelope(false)).unwrap_err().ec(),
            ErrorCode::ResourceBusy
        );
        assert_eq!(held.bytes, encoded);
        drop(held);
        queue.send(envelope(false)).unwrap();
        drop(rx);
        assert_eq!(
            queue.send(envelope(false)).unwrap_err().ec(),
            ErrorCode::ChannelClosed
        );
    }
}
