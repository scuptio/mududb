#![allow(clippy::unwrap_used)]
use super::*;
use crate::log_stream::tests::{descriptor, identity};

fn dir() -> PathBuf {
    mudu_sys::env_var::temp_dir().join(format!("log-image-{}", mudu_sys::random::uuid_v4()))
}
async fn open(path: PathBuf) -> DurableStreamLog {
    DurableStreamLog::open(path, identity(), descriptor(1))
        .await
        .unwrap()
}
async fn source() -> (DurableStreamLog, StreamProgress) {
    let log = open(dir()).await;
    log.append(10, b"first".to_vec(), vec![]).await.unwrap();
    let (cut, _) = log.append(11, b"second".to_vec(), vec![]).await.unwrap();
    (log, cut)
}

#[tokio::test]
async fn restores_retry_results_and_excludes_speculative_tail_across_reopen() {
    let (source, cut) = source().await;
    source
        .append(12, b"speculative".to_vec(), vec![])
        .await
        .unwrap();
    let image = source.capture_snapshot(Some(&cut)).await.unwrap();
    let path = dir();
    let log = open(path.clone()).await;
    log.restore_snapshot(&image).await.unwrap();
    log.restore_snapshot(&image).await.unwrap();
    drop(log);
    let log = open(path).await;
    assert_eq!(log.frontier().await.unwrap(), Some(cut.clone()));
    assert!(log.request_progress(12).await.unwrap().is_none());
    let expected = source.request_progress(10).await.unwrap().unwrap();
    assert_eq!(
        log.append(10, b"first".to_vec(), vec![]).await.unwrap().0,
        expected
    );
    assert!(log.append(10, b"different".to_vec(), vec![]).await.is_err());
    let (next, _) = log.append(13, b"next".to_vec(), vec![]).await.unwrap();
    assert_eq!(next.end_lsn, cut.end_lsn + 1);
    assert!(log.restore_snapshot(&image).await.is_err());
    assert_eq!(log.frontier().await.unwrap(), Some(next));
}

#[tokio::test]
async fn interrupted_restore_resumes_only_from_identical_partial_prefix() {
    let (source, cut) = source().await;
    let image = source.capture_snapshot(Some(&cut)).await.unwrap();
    let path = dir();
    let log = open(path.clone()).await;
    log.receive(&image.frames[0]).await.unwrap();
    drop(log); // partial durable restore, then a fresh process view
    let log = open(path).await;
    log.restore_snapshot(&image).await.unwrap();
    assert_eq!(log.frontier().await.unwrap(), Some(cut));
    let conflicting = open(dir()).await;
    let (before, _) = conflicting
        .append(10, b"unrelated".to_vec(), vec![])
        .await
        .unwrap();
    assert!(conflicting.restore_snapshot(&image).await.is_err());
    assert_eq!(conflicting.frontier().await.unwrap(), Some(before));
    assert!(conflicting.append(99, vec![1], vec![]).await.is_ok());
}

#[tokio::test]
async fn invalid_images_are_checked_completely_before_writing() {
    let (source, cut) = source().await;
    let image = source.capture_snapshot(Some(&cut)).await.unwrap();
    for case in 0..7 {
        let target = open(dir()).await;
        let mut bad = image.clone();
        match case {
            0 => bad.identity.generation += 1,
            1 => bad.frames.swap(0, 1),
            2 => {
                bad.frames.pop();
            }
            3 => bad.prefix.as_mut().unwrap().segment_digest[0] ^= 1,
            4 => bad.frames[1][0] ^= 1,
            5 => bad.version += 1,
            _ => bad.descriptor.stream_generation += 1,
        }
        assert!(target.restore_snapshot(&bad).await.is_err(), "case {case}");
        assert!(target.frontier().await.unwrap().is_none());
        assert!(target.append(100, vec![1], vec![]).await.is_ok());
    }
}

#[tokio::test]
async fn valid_crc_duplicate_request_ids_are_rejected() {
    let (source, cut) = source().await;
    let mut image = source.capture_snapshot(Some(&cut)).await.unwrap();
    let mut body: StreamEnvelope =
        deserialize_entry(std::slice::from_ref(&image.frames[1])).unwrap();
    body.request_id = 10;
    image.frames[1] = serialize_entry(&body, 4096, &AtomicU64::new(1))
        .unwrap()
        .remove(0);
    let digest = seg_hash_chain(&ZERO_SEG_HASH, &image.frames[..1]);
    image.prefix.as_mut().unwrap().segment_digest = seg_hash_chain(&digest, &image.frames[1..]);
    assert!(image
        .validate_for(identity(), &descriptor(1), image.prefix())
        .is_err());
    let log = open(dir()).await;
    assert!(log.restore_snapshot(&image).await.is_err());
    assert!(log.frontier().await.unwrap().is_none());
}

#[tokio::test]
async fn empty_images_and_reclaimed_history_fail_closed() {
    let (source, cut) = source().await;
    let empty = source.capture_snapshot(None).await.unwrap();
    let target = open(dir()).await;
    target.restore_snapshot(&empty).await.unwrap();
    assert!(target.frontier().await.unwrap().is_none());
    assert_eq!(
        target.append(9, vec![0], vec![]).await.unwrap().0.end_lsn,
        0
    );
    assert!(target.restore_snapshot(&empty).await.is_err());
    source.gc_prefix(1).await.unwrap();
    assert!(source.capture_snapshot(Some(&cut)).await.is_err());
    assert!(source.capture_snapshot(None).await.is_err());
}
