use crate::wal::log_frame::decode_entries_with_pending;
use crate::wal::log_frame::{deserialize_entry, serialize_entry};
use crate::wal::lsn::LSN;
use crate::wal::pl_batch::PLBatch;
use crate::wal::typed_worker_log::{TypedWorkerLog, WorkerLogRecoveryHandler};
use crate::wal::worker_log::WorkerLogBackend;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::sync::atomic::AtomicU32;
use async_trait::async_trait;

/// Typed worker-log wrapper specialized for [`PLBatch`].
///
/// This wrapper is intended for physical redo records that describe page-level
/// mutations, such as partial byte updates to a specific page in a file.
pub type PLBatchWorkerLog<B, H> = TypedWorkerLog<PLBatch, B, H>;

/// No-op recovery handler for write-only physical-log paths.
pub struct NoopPLBatchRecoveryHandler;

#[async_trait]
impl WorkerLogRecoveryHandler<PLBatch> for NoopPLBatchRecoveryHandler {
    async fn handle_entry(&self, _entry: PLBatch, _start_lsn: LSN) -> RS<()> {
        Ok(())
    }
}

pub fn new_pl_batch_worker_log<B, H>(backend: B, handler: H) -> PLBatchWorkerLog<B, H>
where
    B: WorkerLogBackend,
    H: WorkerLogRecoveryHandler<PLBatch>,
{
    TypedWorkerLog::new(backend, handler)
}

/// Builds a [`PLBatchWorkerLog`] for append/flush paths.
pub fn new_pl_batch_writer<B>(backend: B) -> PLBatchWorkerLog<B, NoopPLBatchRecoveryHandler>
where
    B: WorkerLogBackend,
{
    TypedWorkerLog::new(backend, NoopPLBatchRecoveryHandler)
}

pub fn serialize_pl_batch(
    batch: &PLBatch,
    max_part_size: usize,
    next_lsn: &AtomicU32,
) -> RS<Vec<Vec<u8>>> {
    serialize_entry(batch, max_part_size, next_lsn)
}

pub fn deserialize_pl_batch(parts: &[Vec<u8>]) -> RS<PLBatch> {
    deserialize_entry(parts)
}

pub fn decode_pl_batches(frames: &[Vec<u8>]) -> RS<Vec<PLBatch>> {
    let mut pending = Vec::new();
    let mut pending_start_lsn = None;
    let batches = decode_pl_batches_with_pending(frames, &mut pending, &mut pending_start_lsn)?;
    if !pending.is_empty() {
        return Err(m_error!(EC::DecodeErr, "trailing partial pl batch frames"));
    }
    Ok(batches)
}

pub fn decode_pl_batches_with_pending(
    frames: &[Vec<u8>],
    pending: &mut Vec<Vec<u8>>,
    pending_start_lsn: &mut Option<LSN>,
) -> RS<Vec<PLBatch>> {
    let mut out: Vec<PLBatch> = Vec::new();
    for (_, batch) in decode_entries_with_pending::<PLBatch>(frames, pending, pending_start_lsn)? {
        out.push(batch);
    }
    Ok(out)
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::wal::log_frame::{
        frame_lsns, split_frame, LOG_FRAME_HEADER_SIZE, LOG_FRAME_TAILER_SIZE,
    };
    use crate::wal::pl_entry::{PLEntry, PLFileId, PLOp, PageUpdate};

    fn sample_batch(entry_count: usize, patch_size: usize) -> PLBatch {
        let mut entries = Vec::with_capacity(entry_count);
        for i in 0..entry_count {
            entries.push(PLEntry {
                file: PLFileId {
                    partition_id: 7,
                    table_id: 100u128 + i as u128,
                    file_index: 0,
                },
                ops: vec![
                    PLOp::Create,
                    PLOp::PageUpdate(PageUpdate {
                        page_id: i as u32,
                        offset: 16,
                        data: vec![i as u8 + 1; patch_size],
                    }),
                ],
            });
        }
        PLBatch::new(entries)
    }

    #[test]
    fn pl_batch_single_part_round_trip() {
        let batch = sample_batch(1, 24);
        let next_lsn = AtomicU32::new(0);
        let parts = serialize_pl_batch(&batch, 4096, &next_lsn).unwrap();
        let lsns = frame_lsns(&parts).unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(lsns, vec![0]);
        let (header, payload, tailer) = split_frame(&parts[0]).unwrap();
        assert_eq!(header.lsn(), 0);
        assert_eq!(header.n_part(), 0);
        assert_eq!(tailer.n_part(), 0);
        assert_eq!(payload.len(), header.size() as usize);
        assert_eq!(deserialize_pl_batch(&parts).unwrap(), batch);
    }

    #[test]
    fn pl_batch_splits_large_payload_into_multiple_parts() {
        let batch = sample_batch(4, 256);
        let next_lsn = AtomicU32::new(7);
        let parts = serialize_pl_batch(&batch, 180, &next_lsn).unwrap();
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
        assert_eq!(deserialize_pl_batch(&parts).unwrap(), batch);
    }

    #[test]
    fn pl_batch_rejects_corrupted_payload_checksum() {
        let batch = sample_batch(1, 32);
        let next_lsn = AtomicU32::new(0);
        let mut parts = serialize_pl_batch(&batch, 4096, &next_lsn).unwrap();
        let idx = parts[0].len() - LOG_FRAME_TAILER_SIZE - 1;
        parts[0][idx] ^= 0x7f;
        let err = deserialize_pl_batch(&parts).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("checksum"), "{}", msg);
    }

    #[test]
    fn pl_batch_rejects_invalid_part_size_configuration() {
        let batch = sample_batch(1, 8);
        let next_lsn = AtomicU32::new(0);
        let err = serialize_pl_batch(
            &batch,
            LOG_FRAME_HEADER_SIZE + LOG_FRAME_TAILER_SIZE,
            &next_lsn,
        )
        .unwrap_err();
        assert!(err.to_string().contains("max_part_size"));
    }
}
