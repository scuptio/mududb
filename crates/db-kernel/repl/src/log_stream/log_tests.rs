//! Real-file external stream durability and recovery tests.
#![allow(clippy::unwrap_used)]
use super::tests::{descriptor, identity};
use super::*;
use mudu_raft::stream::StreamProgress;

#[tokio::test]
async fn external_log_retries_restart_and_authoritative_tail_truncation() {
    let dir =
        mudu_sys::env_var::temp_dir().join(format!("stream-log-{}", mudu_sys::random::uuid_v4()));
    let log = DurableStreamLog::open(dir.clone(), identity(), descriptor(1))
        .await
        .unwrap();
    let (first, bytes) = log.append(10, b"catalog".to_vec(), vec![]).await.unwrap();
    assert_eq!(
        log.append(10, b"catalog".to_vec(), vec![]).await.unwrap(),
        (first.clone(), bytes)
    );
    assert!(log.append(10, b"collision".to_vec(), vec![]).await.is_err());
    let (tail, _) = log
        .append(11, b"speculative".to_vec(), vec![])
        .await
        .unwrap();
    drop(log);
    let log = DurableStreamLog::open(dir.clone(), identity(), descriptor(1))
        .await
        .unwrap();
    assert!(log.contains(&first).await.unwrap());
    assert!(log.contains(&tail).await.unwrap());
    assert_eq!(log.record(0).await.unwrap().1.payload, b"catalog");
    log.truncate_uncommitted(Some(&first)).await.unwrap();
    assert!(log.request_progress(11).await.unwrap().is_none());
    let (replacement, _) = log
        .append(12, b"replacement".to_vec(), vec![])
        .await
        .unwrap();
    assert_eq!(replacement.end_lsn, tail.end_lsn);
    assert_ne!(replacement.segment_digest, tail.segment_digest);
    drop(log);
    let log = DurableStreamLog::open(dir.clone(), identity(), descriptor(1))
        .await
        .unwrap();
    assert!(log.contains(&replacement).await.unwrap());
    assert!(!log.contains(&tail).await.unwrap());
    drop(log);
    assert!(
        DurableStreamLog::open(dir.clone(), identity(), descriptor(2))
            .await
            .is_err()
    );
    mudu_sys::fs::tokio_fs().remove_dir_all(&dir).await.unwrap();
}

#[tokio::test]
async fn peer_frames_reject_gaps_corruption_and_foreign_streams() {
    let base =
        mudu_sys::env_var::temp_dir().join(format!("stream-peers-{}", mudu_sys::random::uuid_v4()));
    let a = DurableStreamLog::open(base.join("a"), identity(), descriptor(1))
        .await
        .unwrap();
    let b = DurableStreamLog::open(base.join("b"), identity(), descriptor(1))
        .await
        .unwrap();
    let foreign = DurableStreamLog::open(base.join("foreign"), identity(), descriptor(2))
        .await
        .unwrap();
    let (first, frame0) = a.append(1, vec![7; 1000], vec![]).await.unwrap();
    let (last, frame1) = a.append(2, vec![8; 1000], vec![]).await.unwrap();
    assert!(b.receive(&frame1).await.is_err());
    assert!(foreign.receive(&frame0).await.is_err());
    let mut corrupt = frame0.clone();
    corrupt[40] ^= 1;
    assert!(b.receive(&corrupt).await.is_err());
    assert_eq!(b.receive(&frame0).await.unwrap(), first);
    assert_eq!(b.receive(&frame0).await.unwrap(), first);
    assert_eq!(b.receive(&frame1).await.unwrap(), last);
    assert!(b.contains(&last).await.unwrap());
    assert!(a.append(3, vec![0; 1025], vec![]).await.is_err());
    drop((a, b, foreign));
    mudu_sys::fs::tokio_fs()
        .remove_dir_all(&base)
        .await
        .unwrap();
}

#[tokio::test]
async fn gc_prefix_reclaims_history_and_survives_restart() {
    let dir =
        mudu_sys::env_var::temp_dir().join(format!("stream-gc-{}", mudu_sys::random::uuid_v4()));
    let log = DurableStreamLog::open(dir.clone(), identity(), descriptor(1))
        .await
        .unwrap();
    let mut progress = Vec::new();
    for i in 0..6 {
        progress.push(
            log.append(100 + i, format!("record-{i}").into_bytes(), vec![])
                .await
                .unwrap()
                .0,
        );
    }
    // Reclaim records 0..4 (retain 4..6): the anchor is the digest at lsn 3.
    assert_eq!(log.gc_prefix(4).await.unwrap(), 0);
    // Below-frontier reads fail closed; the anchor prefix matches by digest.
    assert!(log.record(3).await.is_err());
    assert!(!log.contains(&progress[2]).await.unwrap());
    let anchor = StreamProgress {
        stream_id: progress[3].stream_id,
        stream_generation: progress[3].stream_generation,
        end_lsn: 3,
        segment_digest: progress[3].segment_digest,
    };
    assert!(log.contains(&anchor).await.unwrap());
    assert_eq!(log.record(4).await.unwrap().1.payload, b"record-4");
    assert_eq!(log.progress_at(5).await.unwrap().as_ref(), progress.last());
    // The chain continues from the anchor, not from zero.
    let (next, _) = log.append(200, b"after-gc".to_vec(), vec![]).await.unwrap();
    assert_eq!(next.end_lsn, 6);
    // Receiving an already-retained frame is idempotent (its own prefix).
    let retained = log.read_frame(4).await.unwrap();
    assert_eq!(log.receive(&retained).await.unwrap().end_lsn, 4);
    // Restart: the anchor drives the rebuilt index.
    drop(log);
    let log = DurableStreamLog::open(dir.clone(), identity(), descriptor(1))
        .await
        .unwrap();
    assert!(log.record(0).await.is_err());
    assert_eq!(log.record(4).await.unwrap().1.payload, b"record-4");
    assert_eq!(log.record(6).await.unwrap().1.payload, b"after-gc");
    assert!(log.contains(&next).await.unwrap());
    assert!(log.frontier().await.unwrap().unwrap().end_lsn == 6);
    drop(log);
    mudu_sys::fs::tokio_fs().remove_dir_all(&dir).await.unwrap();
}
