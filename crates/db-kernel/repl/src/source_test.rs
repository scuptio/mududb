#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;
use ::wal::worker_log::{decode_frames, WorkerLogLayout};
use ::wal::xl_batch::{append_xl_batch_async, XLBatch};
use ::wal::xl_data_op::{XLInsert, XLWrite};
use ::wal::xl_entry::{TxOp, XLEntry};
use mudu_sys::env_var::temp_dir;
use mudu_utils::oid::gen_oid;

fn batch_with_payload(xid: u64, payload_size: usize) -> XLBatch {
    XLBatch::new(vec![XLEntry {
        xid,
        ops: vec![
            TxOp::Begin,
            TxOp::Write(XLWrite::Insert(XLInsert {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key: format!("k{xid}").into_bytes(),
                value: vec![xid as u8; payload_size],
            })),
            TxOp::Commit(Default::default()),
        ],
        commit_epoch: None,
    }])
}

async fn log_with_batches(payload_sizes: &[usize]) -> (WorkerLogLayout, ChunkedWorkerLogBackend) {
    let dir = temp_dir().join(format!("repl_source_test_{}", gen_oid()));
    let layout = WorkerLogLayout::new(dir, gen_oid(), 4096).unwrap();
    let log = ChunkedWorkerLogBackend::new(layout.clone()).await.unwrap();
    for (index, size) in payload_sizes.iter().enumerate() {
        append_xl_batch_async(&log, &batch_with_payload(index as u64 + 1, *size))
            .await
            .unwrap();
    }
    log.flush_async().await.unwrap();
    (layout, log)
}

/// LSNs of all frames persisted in the log, in log order.
async fn frame_lsns(log: &ChunkedWorkerLogBackend) -> Vec<u64> {
    let mut lsns = Vec::new();
    for path in log.chunk_paths_sorted().await.unwrap() {
        let bytes = log.fs().read_all(&path).await.unwrap();
        for frame in decode_frames(&bytes).unwrap() {
            lsns.push(frame_lsn(&frame).unwrap().as_u64());
        }
    }
    lsns
}

/// Drains the cursor, returning every streamed (lsn_lo, frame-count) batch.
async fn drain(cursor: &mut ChunkCursor) -> Vec<(u64, usize)> {
    let mut batches = Vec::new();
    while let Some((lsn_lo, frames)) = cursor.next_batch().await.unwrap() {
        batches.push((lsn_lo, frames.len()));
    }
    batches
}

#[test]
fn chunk_cursor_streams_all_frames_then_reports_caught_up() {
    mudu_sys::task::async_::block_on_async_current(async move {
        let (_layout, log) = log_with_batches(&[16, 16, 16]).await;
        let total = frame_lsns(&log).await.len();
        let mut cursor = ChunkCursor::new(log, 0);
        let batches = drain(&mut cursor).await;
        let streamed: usize = batches.iter().map(|(_, count)| count).sum();
        assert_eq!(streamed, total);
        assert!(batches.iter().all(|(_, count)| *count > 0));
    });
}

#[test]
fn chunk_cursor_skips_frames_below_from_lsn() {
    mudu_sys::task::async_::block_on_async_current(async move {
        let (_layout, log) = log_with_batches(&[16, 16, 16]).await;
        let lsns = frame_lsns(&log).await;
        assert!(lsns.len() >= 2);
        let from = lsns[1];
        let mut cursor = ChunkCursor::new(log, from);
        let batches = drain(&mut cursor).await;
        // Frames below `from` are filtered; the rest may arrive in one batch.
        let streamed: usize = batches.iter().map(|(_, count)| count).sum();
        assert_eq!(streamed, lsns.len() - 1);
        assert!(batches.iter().all(|(lsn_lo, _)| *lsn_lo >= from));
    });
}

#[test]
fn chunk_cursor_follows_chunk_rotation() {
    mudu_sys::task::async_::block_on_async_current(async move {
        // 3 KB payloads rotate the 4 KB chunks quickly.
        let (_layout, log) = log_with_batches(&[3000; 6]).await;
        let chunk_count = log.chunk_paths_sorted().await.unwrap().len();
        assert!(chunk_count >= 2, "expected rotation, got {chunk_count}");
        let total = frame_lsns(&log).await.len();
        let mut cursor = ChunkCursor::new(log, 0);
        let batches = drain(&mut cursor).await;
        let streamed: usize = batches.iter().map(|(_, count)| count).sum();
        assert_eq!(streamed, total);
    });
}

#[test]
fn chunk_cursor_empty_log_reports_caught_up() {
    mudu_sys::task::async_::block_on_async_current(async move {
        let (_layout, log) = log_with_batches(&[]).await;
        let mut cursor = ChunkCursor::new(log, 0);
        assert!(cursor.next_batch().await.unwrap().is_none());
    });
}

#[test]
fn chunk_cursor_errors_when_current_chunk_disappears() {
    mudu_sys::task::async_::block_on_async_current(async move {
        let (_layout, log) = log_with_batches(&[16]).await;
        let mut cursor = ChunkCursor::new(log, 0);
        cursor.current_path = Some(PathBuf::from("/no/such/chunk"));
        let err = cursor.next_batch().await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
    });
}
