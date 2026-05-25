use crate::wal::log_frame::decode_entries_with_pending;
use crate::wal::log_frame::{deserialize_entry, serialize_entry};
use crate::wal::lsn::LSN;
use crate::wal::typed_worker_log::{TypedWorkerLog, WorkerLogRecoveryHandler};
use crate::wal::worker_log::WorkerLogBackend;
use crate::wal::xl_batch::XLBatch;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::sync::atomic::AtomicU32;

/// Typed worker-log wrapper specialized for [`XLBatch`].
///
/// Typical write path:
///
/// ```ignore
/// let writer = new_xl_batch_writer(log_backend.clone());
/// writer.append_sync(&batch)?;
/// writer.flush()?;
/// ```
///
/// Typical recovery path:
///
/// ```ignore
/// struct RecoveryHandler {
///     worker: WorkerRuntime,
/// }
///
/// impl WorkerLogRecoveryHandler<XLBatch> for RecoveryHandler {
///     fn handle_entry(&self, entry: XLBatch, _start_lsn: LSN) -> RS<()> {
///         self.worker.replay_log_batch(entry)
///     }
/// }
///
/// let typed_log = new_xl_batch_worker_log(log_backend.clone(), RecoveryHandler { worker });
/// typed_log.recover(&mut recovery_source)?;
/// ```
pub type XLBatchWorkerLog<B, H> = TypedWorkerLog<XLBatch, B, H>;

/// No-op recovery handler for write-only paths.
///
/// Use this when the caller only needs typed append/flush APIs and does not
/// plan to invoke `recover(...)` on the wrapper instance.
pub struct NoopXLBatchRecoveryHandler;

impl WorkerLogRecoveryHandler<XLBatch> for NoopXLBatchRecoveryHandler {
    fn handle_entry(&self, _entry: XLBatch, _start_lsn: LSN) -> RS<()> {
        Ok(())
    }
}

pub fn new_xl_batch_worker_log<B, H>(backend: B, handler: H) -> XLBatchWorkerLog<B, H>
where
    B: WorkerLogBackend,
    H: WorkerLogRecoveryHandler<XLBatch>,
{
    TypedWorkerLog::new(backend, handler)
}

/// Builds an [`XLBatchWorkerLog`] for append/flush paths.
///
/// This is the usual choice for commit/write flows, where the caller wants the
/// typed `XLBatch` append APIs but does not need a recovery handler.
pub fn new_xl_batch_writer<B>(backend: B) -> XLBatchWorkerLog<B, NoopXLBatchRecoveryHandler>
where
    B: WorkerLogBackend,
{
    TypedWorkerLog::new(backend, NoopXLBatchRecoveryHandler)
}

pub fn serialize_batch(
    batch: &XLBatch,
    max_part_size: usize,
    next_lsn: &AtomicU32,
) -> RS<Vec<Vec<u8>>> {
    serialize_entry(batch, max_part_size, next_lsn)
}

pub fn deserialize_batch(parts: &[Vec<u8>]) -> RS<XLBatch> {
    deserialize_entry(parts)
}

pub fn decode_xl_batches(frames: &[Vec<u8>]) -> RS<Vec<XLBatch>> {
    let mut pending = Vec::new();
    let mut pending_start_lsn = None;
    let batches = decode_xl_batches_with_pending(frames, &mut pending, &mut pending_start_lsn)?;
    if !pending.is_empty() {
        return Err(m_error!(EC::DecodeErr, "trailing partial xl batch frames"));
    }
    Ok(batches)
}

pub fn decode_xl_batches_with_pending(
    frames: &[Vec<u8>],
    pending: &mut Vec<Vec<u8>>,
    pending_start_lsn: &mut Option<LSN>,
) -> RS<Vec<XLBatch>> {
    let mut out: Vec<XLBatch> = Vec::new();
    for (_, batch) in decode_entries_with_pending::<XLBatch>(frames, pending, pending_start_lsn)? {
        out.push(batch);
    }
    Ok(out)
}

pub fn append_xl_batch<B: WorkerLogBackend>(backend: &B, batch: &XLBatch) -> RS<()> {
    let frames = backend.serialize_entry(batch)?;
    backend.append_frames_sync(frames)
}

pub async fn append_xl_batch_async<B: WorkerLogBackend>(backend: &B, batch: &XLBatch) -> RS<LSN> {
    let frames = backend.serialize_entry(batch)?;
    backend.append_frames_async(frames).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wal::log_frame::{
        frame_lsns, split_frame, LOG_FRAME_HEADER_SIZE, LOG_FRAME_TAILER_SIZE,
    };
    use crate::wal::xl_data_op::XLInsert;
    use crate::wal::xl_entry::{TxOp, XLEntry};

    fn sample_batch(entry_count: usize, payload_size: usize) -> XLBatch {
        let mut entries = Vec::with_capacity(entry_count);
        for xid in 0..entry_count {
            entries.push(XLEntry {
                xid: xid as u64 + 1,
                ops: vec![
                    TxOp::Begin,
                    TxOp::Insert(XLInsert {
                        table_id: 7,
                        partition_id: 0,
                        tuple_id: xid as u64 + 10,
                        key: format!("key-{xid}").into_bytes(),
                        value: vec![xid as u8; payload_size],
                    }),
                    TxOp::Commit,
                ],
            });
        }
        XLBatch::new(entries)
    }

    #[test]
    fn xl_batch_single_part_round_trip() {
        let batch = sample_batch(1, 32);
        let next_lsn = AtomicU32::new(0);
        let parts = serialize_batch(&batch, 4096, &next_lsn).unwrap();
        let lsns = frame_lsns(&parts).unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(lsns, vec![0]);
        let (header, payload, tailer) = split_frame(&parts[0]).unwrap();
        assert_eq!(header.lsn(), 0);
        assert_eq!(header.n_part(), 0);
        assert_eq!(tailer.n_part(), 0);
        assert_eq!(payload.len(), header.size() as usize);
        assert_eq!(deserialize_batch(&parts).unwrap(), batch);
    }

    #[test]
    fn xl_batch_splits_large_payload_into_multiple_parts() {
        let batch = sample_batch(4, 256);
        let next_lsn = AtomicU32::new(10);
        let parts = serialize_batch(&batch, 180, &next_lsn).unwrap();
        let lsns = frame_lsns(&parts).unwrap();
        assert!(parts.len() > 1);
        assert_eq!(lsns.len(), parts.len());
        for (index, part) in parts.iter().enumerate() {
            assert!(part.len() <= 180);
            let (header, _, tailer) = split_frame(part).unwrap();
            let expected = (parts.len() - index - 1) as u32;
            assert_eq!(header.lsn(), lsns[index]);
            assert_eq!(header.n_part(), expected);
            assert_eq!(tailer.n_part(), expected);
        }
        assert_eq!(deserialize_batch(&parts).unwrap(), batch);
    }

    #[test]
    fn xl_batch_rejects_corrupted_payload_checksum() {
        let batch = sample_batch(1, 32);
        let next_lsn = AtomicU32::new(0);
        let mut parts = serialize_batch(&batch, 4096, &next_lsn).unwrap();
        let idx = parts[0].len() - LOG_FRAME_TAILER_SIZE - 1;
        parts[0][idx] ^= 0x7f;
        let err = deserialize_batch(&parts).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("checksum"), "{}", msg);
    }

    #[test]
    fn xl_batch_rejects_part_order_mismatch() {
        let batch = sample_batch(4, 256);
        let next_lsn = AtomicU32::new(0);
        let mut parts = serialize_batch(&batch, 180, &next_lsn).unwrap();
        parts.swap(0, 1);
        let err = deserialize_batch(&parts).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unexpected log frame order") || msg.contains("checksum"),
            "{}",
            msg
        );
    }

    #[test]
    fn xl_batch_rejects_invalid_part_size_configuration() {
        let batch = sample_batch(1, 8);
        let next_lsn = AtomicU32::new(0);
        let err = serialize_batch(
            &batch,
            LOG_FRAME_HEADER_SIZE + LOG_FRAME_TAILER_SIZE,
            &next_lsn,
        )
        .unwrap_err();
        assert!(err.to_string().contains("max_part_size"));
    }
}
