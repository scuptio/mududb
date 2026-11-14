#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;
use ::wal::log_frame::serialize_entry;
use ::wal::worker_log::WorkerLogLayout;
use mudu_contract::protocol::{
    encode_repl_snapshot_offer_frame, LeaderHint, ReplSnapshotOfferFrame,
};
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::default_sys_io_context;
use mudu_sys::env_var::temp_dir;
use mudu_utils::oid::gen_oid;
use std::sync::atomic::AtomicU64;

fn make_frames(start_lsn: u64, count: u64) -> Vec<Vec<u8>> {
    let next = AtomicU64::new(start_lsn);
    let mut out = Vec::new();
    for i in 0..count {
        let parts = serialize_entry(&format!("value-{i}"), 4096, &next).unwrap();
        out.extend(parts);
    }
    out
}

fn make_frame(lsn: u64, content: &str) -> Vec<u8> {
    serialize_entry(&content.to_string(), 4096, &AtomicU64::new(lsn))
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
}

fn test_fs() -> Arc<dyn AsyncFs> {
    default_sys_io_context().provider_arc().fs_arc()
}

async fn test_staging() -> Arc<StagingStore> {
    let dir = temp_dir().join(format!("sink-stage-test-{}", gen_oid()));
    Arc::new(StagingStore::open(dir, test_fs()).await.unwrap())
}

#[test]
fn stage_frames_appends_and_tracks_progress() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let staging = test_staging().await;
        let mut staged = StagingTrack {
            next_expected: 0,
            staged_hi: None,
        };
        stage_frames(&staging, make_frames(0, 3), &mut staged)
            .await
            .unwrap();
        assert_eq!(staged.next_expected, 3);
        assert_eq!(staged.staged_hi, Some(2));
        assert_eq!(staging.max_lsn().unwrap(), Some(LSN::new(2)));
    })
    .unwrap()
}

#[test]
fn stage_frames_skips_byte_identical_overlap() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let staging = test_staging().await;
        let mut staged = StagingTrack {
            next_expected: 0,
            staged_hi: None,
        };
        stage_frames(&staging, make_frames(0, 3), &mut staged)
            .await
            .unwrap();

        // A resubscription re-streams lsns 1 and 2 with identical bytes:
        // the overlap is skipped, nothing is truncated or re-appended.
        stage_frames(&staging, make_frames(1, 2), &mut staged)
            .await
            .unwrap();
        assert_eq!(staged.next_expected, 3);
        assert_eq!(staged.staged_hi, Some(2));
        assert_eq!(staging.max_lsn().unwrap(), Some(LSN::new(2)));
    })
    .unwrap()
}

#[test]
fn stage_frames_truncates_divergent_tail() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let staging = test_staging().await;
        let mut staged = StagingTrack {
            next_expected: 0,
            staged_hi: None,
        };
        stage_frames(&staging, make_frames(0, 3), &mut staged)
            .await
            .unwrap();

        // The leader streams different content at lsn 1: the speculative
        // staged tail is truncated from there and replaced.
        let divergent = vec![make_frame(1, "leader-content")];
        stage_frames(&staging, divergent.clone(), &mut staged)
            .await
            .unwrap();
        assert_eq!(staged.next_expected, 2);
        assert_eq!(staged.staged_hi, Some(1));
        assert_eq!(staging.max_lsn().unwrap(), Some(LSN::new(1)));
        assert_eq!(
            staging.frame_bytes(1).await.unwrap().as_deref(),
            Some(divergent[0].as_slice())
        );
    })
    .unwrap()
}

#[test]
fn stage_frames_rejects_a_stream_gap() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let staging = test_staging().await;
        let mut staged = StagingTrack {
            next_expected: 0,
            staged_hi: None,
        };
        let err = stage_frames(&staging, make_frames(5, 1), &mut staged)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidState);
        assert!(err.to_string().contains("stream gap"), "unexpected: {err}");
    })
    .unwrap()
}

#[test]
fn scan_local_crcs_maps_only_the_probe_window() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = temp_dir().join(format!("sink-crc-test-{}", gen_oid()));
        let layout = WorkerLogLayout::new(dir, gen_oid(), 4096).unwrap();
        let log = ChunkedWorkerLogBackend::new(layout).await.unwrap();
        let frames = make_frames(0, 4);
        log.append_frames_async(frames.clone()).await.unwrap();
        log.flush_async().await.unwrap();

        let crcs = scan_local_crcs(&log, 1, 3).await.unwrap();
        assert_eq!(crcs.len(), 2);
        assert_eq!(crcs[&1], crc64(&frames[1]));
        assert_eq!(crcs[&2], crc64(&frames[2]));

        // An empty or inverted window scans nothing.
        assert!(scan_local_crcs(&log, 3, 3).await.unwrap().is_empty());
        assert!(scan_local_crcs(&log, 4, 2).await.unwrap().is_empty());
    })
    .unwrap()
}

fn test_manifest() -> ReplSnapshotManifest {
    ReplSnapshotManifest {
        version: 1,
        worker_index: 0,
        committed_lsn: 41,
        seg_hash: [0; 32],
        raft_term: 0,
        raft_index: 0,
        taken_at_ms: 0,
        files: vec![],
    }
}

#[test]
fn snapshot_offer_decodes_for_the_matching_worker() {
    let offer = ReplSnapshotOfferFrame::new(0, test_manifest());
    let bytes = encode_repl_snapshot_offer_frame(1, &offer).unwrap();
    let (frame, _) = crate::try_decode_next_frame(&bytes).unwrap().unwrap();
    match decode_snapshot_offer(&frame, 0).unwrap() {
        StreamOutcome::Offered(manifest) => assert_eq!(manifest.committed_lsn, 41),
        StreamOutcome::Interrupted => panic!("expected a snapshot offer outcome"),
    }
}

#[test]
fn snapshot_offer_for_another_worker_is_an_error() {
    let offer = ReplSnapshotOfferFrame::new(0, test_manifest());
    let bytes = encode_repl_snapshot_offer_frame(1, &offer).unwrap();
    let (frame, _) = crate::try_decode_next_frame(&bytes).unwrap().unwrap();
    let err = decode_snapshot_offer(&frame, 7).err().unwrap();
    assert_eq!(err.ec(), ErrorCode::Parse);
    assert!(
        err.to_string().contains("arrived on worker 7"),
        "unexpected: {err}"
    );
}

#[test]
fn remote_error_message_appends_the_leader_hint() {
    let hinted =
        ErrorResponse::new("not the leader").with_leader_hint(LeaderHint::new("10.0.0.2", 9700));
    let err = repl_remote_error(&hinted);
    assert_eq!(err.ec(), ErrorCode::Internal);
    let message = err.to_string();
    assert!(message.contains("not the leader"), "unexpected: {message}");
    assert!(message.contains("10.0.0.2:9700"), "unexpected: {message}");

    let plain = repl_remote_error(&ErrorResponse::new("plain failure"));
    assert!(plain.to_string().contains("plain failure"));
    assert!(!plain.to_string().contains("leader hint"));
}
