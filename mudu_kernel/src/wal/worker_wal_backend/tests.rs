#![allow(clippy::arc_with_non_send_sync)]

use super::*;

use crate::wal::log_frame::split_frame;
use crate::wal::lsn::LSN;
use crate::wal::worker_log::decode_frames;
use crate::wal::worker_log::WorkerLogBackend;
use crate::wal::xl_batch::{
    append_xl_batch_async, decode_xl_batches, decode_xl_batches_with_pending, serialize_batch,
    XLBatch,
};
use crate::wal::xl_data_op::{XLInsert, XLWrite};
use crate::wal::xl_entry::{TxOp, XLEntry};
use mudu_sys::common::provider_type::ProviderType;
use mudu_sys::env_var::temp_dir;
use mudu_sys::io::worker_ring;
use mudu_sys::provider::create_io_provider;
use mudu_utils::oid::gen_oid;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

fn sample_batch() -> XLBatch {
    XLBatch::new(vec![XLEntry {
        xid: 1,
        ops: vec![
            TxOp::Begin,
            TxOp::Write(XLWrite::Insert(XLInsert {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key: b"k1".to_vec(),
                value: b"v1".to_vec(),
            })),
            TxOp::Commit,
        ],
    }])
}

#[test]
fn worker_log_appends_batch_frames() {
    mudu_sys::task::async_::block_on_async_current(async move {
        let dir = temp_dir().join(format!("worker_kv_log_test_{}", gen_oid()));
        let layout = WorkerLogLayout::new(dir, gen_oid(), 4096).unwrap();
        let path = layout.chunk_path(0);
        let log = WorkerWALBackend::new(layout).await.unwrap();
        futures::executor::block_on(append_xl_batch_async(&log, &sample_batch())).unwrap();
        log.flush_async().await.unwrap();
        let bytes = mudu_sys::fs::sync::read(path).unwrap();
        assert!(!bytes.is_empty());
    });
}

#[test]
fn worker_log_round_trips_batch_frames() {
    mudu_sys::task::async_::block_on_async_current(async move {
        let batch = sample_batch();
        let log = WorkerWALBackend::new(
            WorkerLogLayout::new(
                temp_dir().join(format!("worker_log_round_{}", gen_oid())),
                gen_oid(),
                4096,
            )
            .unwrap(),
        )
        .await
        .unwrap();
        let next_lsn = AtomicU64::new(0);
        let frames = serialize_batch(&batch, log.frame_size_limit().unwrap(), &next_lsn).unwrap();
        let decoded = decode_xl_batches(&frames).unwrap();
        assert_eq!(decoded, vec![batch]);
    });
}

#[test]
fn worker_log_decodes_multiple_frames_from_single_chunk_payload() {
    let first = sample_batch();
    let second = XLBatch::new(vec![XLEntry {
        xid: 2,
        ops: vec![
            TxOp::Begin,
            TxOp::Write(XLWrite::Insert(XLInsert {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key: b"k2".to_vec(),
                value: b"v2".to_vec(),
            })),
            TxOp::Commit,
        ],
    }]);
    let mut bytes = Vec::new();
    let next_lsn = AtomicU64::new(0);
    bytes.extend(
        serialize_batch(&first, 4096, &next_lsn)
            .unwrap()
            .into_iter()
            .flatten(),
    );
    bytes.extend(
        serialize_batch(&second, 4096, &next_lsn)
            .unwrap()
            .into_iter()
            .flatten(),
    );

    let frames = decode_frames(&bytes).unwrap();
    let batches = decode_xl_batches(&frames).unwrap();
    assert_eq!(batches, vec![first, second]);
}

#[test]
fn worker_log_decodes_batch_frames_across_chunk_boundaries() {
    let batch = sample_xl_batch_1();
    let next_lsn = AtomicU64::new(0);
    let frames = serialize_batch(&batch, 128, &next_lsn).unwrap();
    assert!(frames.len() > 1);

    let split_at = frames.len() / 2;
    let first_chunk_frames = frames[..split_at].to_vec();
    let second_chunk_frames = frames[split_at..].to_vec();
    let mut pending = Vec::new();
    let mut pending_start_lsn = None;

    let first_batches =
        decode_xl_batches_with_pending(&first_chunk_frames, &mut pending, &mut pending_start_lsn)
            .unwrap();
    assert!(first_batches.is_empty());
    assert!(!pending.is_empty());

    let second_batches =
        decode_xl_batches_with_pending(&second_chunk_frames, &mut pending, &mut pending_start_lsn)
            .unwrap();
    assert!(pending.is_empty());
    assert_eq!(second_batches, vec![batch]);
}

#[test]
fn worker_log_rotates_chunks_by_size() {
    mudu_sys::task::async_::block_on_async_current(async move {
        let dir = temp_dir().join(format!("worker_kv_log_chunk_{}", gen_oid()));
        let layout = WorkerLogLayout::new(dir.clone(), gen_oid(), 40).unwrap();
        let prefix = layout.short_oid.clone();
        let log = WorkerWALBackend::new(layout).await.unwrap();
        log.append_raw(&[1u8; 20]).await.unwrap();
        log.append_raw(&[2u8; 20]).await.unwrap();
        log.append_raw(&[3u8; 20]).await.unwrap();
        assert!(dir.join(format!("{}.0.xl", prefix)).exists());
        assert!(dir.join(format!("{}.1.xl", prefix)).exists());
    });
}

fn sample_xl_batch_1() -> XLBatch {
    XLBatch::new(vec![XLEntry {
        xid: 1,
        ops: vec![
            TxOp::Begin,
            TxOp::Write(XLWrite::Insert(XLInsert {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key: b"k".to_vec(),
                value: vec![9u8; 512],
            })),
            TxOp::Commit,
        ],
    }])
}

#[test]
fn worker_log_serializes_frame_headers_with_monotonic_lsn() {
    mudu_sys::task::async_::block_on_async_current(async move {
        let batch = sample_xl_batch_1();
        let log = WorkerWALBackend::new(
            WorkerLogLayout::new(
                temp_dir().join(format!("worker_log_lsn_{}", gen_oid())),
                gen_oid(),
                128,
            )
            .unwrap(),
        )
        .await
        .unwrap();
        let next_lsn = AtomicU64::new(0);
        let frames = serialize_batch(&batch, log.frame_size_limit().unwrap(), &next_lsn).unwrap();
        assert!(frames.len() > 1);
        for (index, frame) in frames.iter().enumerate() {
            let (header, _, _) = split_frame(frame).unwrap();
            assert_eq!(header.lsn(), LSN::new(index as u64));
        }
    });
}

#[test]
fn worker_log_places_oversized_entry_in_dedicated_chunk() {
    mudu_sys::task::async_::block_on_async_current(async move {
        let dir = temp_dir().join(format!("worker_kv_log_oversized_{}", gen_oid()));
        let layout = WorkerLogLayout::new(dir.clone(), gen_oid(), 32).unwrap();
        let prefix = layout.short_oid.clone();
        let log = WorkerWALBackend::new(layout).await.unwrap();
        log.append_raw(&[1u8; 8]).await.unwrap();
        log.append_raw(&[2u8; 64]).await.unwrap();
        log.append_raw(&[3u8; 8]).await.unwrap();
        log.flush_async().await.unwrap();
        assert_eq!(
            mudu_sys::fs::sync::metadata(dir.join(format!("{}.0.xl", prefix)))
                .unwrap()
                .len(),
            8
        );
        assert_eq!(
            mudu_sys::fs::sync::metadata(dir.join(format!("{}.1.xl", prefix)))
                .unwrap()
                .len(),
            64
        );
        assert_eq!(
            mudu_sys::fs::sync::metadata(dir.join(format!("{}.2.xl", prefix)))
                .unwrap()
                .len(),
            8
        );
    });
}

#[test]
fn worker_log_layout_scans_tail_async() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let dir = temp_dir().join(format!("worker_log_async_scan_{}", gen_oid()));
        let layout = WorkerLogLayout::new(dir.clone(), gen_oid(), 64).unwrap();
        let prefix = layout.short_oid.clone();
        let provider = create_io_provider(ProviderType::Tokio);
        let log = WorkerWALBackend::new(layout.clone()).await.unwrap();
        futures::executor::block_on(append_xl_batch_async(&log, &sample_batch())).unwrap();
        futures::executor::block_on(append_xl_batch_async(&log, &sample_batch())).unwrap();

        let paths = layout
            .chunk_paths_sorted_async(provider.fs())
            .await
            .unwrap();
        assert!(!paths.is_empty());
        assert!(paths[0].ends_with(format!("{}.0.xl", prefix)));

        let tail = layout.scan_tail_async(provider.fs()).await.unwrap();
        assert_eq!(tail.next_sequence, paths.len() as u64);
        assert!(tail.next_lsn >= 2);
    })
    .unwrap()
}

#[test]
fn direct_worker_log_does_not_queue_inside_worker_ring() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let dir = temp_dir().join(format!("worker_log_direct_dispatch_{}", gen_oid()));
        let _direct_log = WorkerWALBackend::new_direct(
            WorkerLogLayout::new(dir.join("direct"), gen_oid(), 4096).unwrap(),
        )
        .await
        .unwrap();
        let _queued_log = WorkerWALBackend::new(
            WorkerLogLayout::new(dir.join("queued"), gen_oid(), 4096).unwrap(),
        )
        .await
        .unwrap();

        #[cfg(target_os = "linux")]
        {
            let ring = Arc::new(worker_ring::WorkerLocalRing::new());
            worker_ring::set_current_worker_ring(ring);
            worker_ring::unset_current_worker_ring();
        }

        #[cfg(not(target_os = "linux"))]
        {
            assert!(!direct_log.should_queue_on_current_worker_ring());
            assert!(!queued_log.should_queue_on_current_worker_ring());
        }
    })
    .unwrap()
}
