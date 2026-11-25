#![allow(clippy::unwrap_used)]
use super::tests::{bundle, dir};
use super::*;
use crate::log_stream::tests::{adapters, apply, change, commit, descriptor, identity};
use crate::log_stream::{DurableStreamLog, LoggedStreamSnapshot};
use mudu_raft::stream::RegistryOp;

async fn logged_bundle(bad_dependency: bool) -> GroupSnapshotBundle {
    let root = dir();
    let mut state = GroupState::new(identity()).unwrap();
    let mut second = descriptor(2);
    second.requires.push(StreamId(1));
    for d in [descriptor(1), second.clone()] {
        let entry = change(&state, d.stream_id.0 as u128, RegistryOp::Register(d));
        apply(&mut state, &entry).unwrap();
    }
    let first = DurableStreamLog::open(root.join("first"), identity(), descriptor(1))
        .await
        .unwrap();
    let second = DurableStreamLog::open(root.join("second"), identity(), second)
        .await
        .unwrap();
    let (p1, _) = first.append(10, b"schema".to_vec(), vec![]).await.unwrap();
    let mut dependency = p1.clone();
    if bad_dependency {
        dependency.segment_digest[0] ^= 1;
    }
    let (p2, _) = second
        .append(11, b"data".to_vec(), vec![dependency])
        .await
        .unwrap();
    let entry = commit(&state, vec![p1.clone(), p2.clone()]);
    apply(&mut state, &entry).unwrap();
    let mut result = bundle();
    result.manifest.state = state;
    result.images.clear();
    result.manifest.images.clear();
    for (log, prefix) in [(first, p1), (second, p2)] {
        let bytes = LoggedStreamSnapshot::encode(
            log.capture_snapshot(Some(&prefix)).await.unwrap(),
            b"business".to_vec(),
        )
        .unwrap();
        let id = log.descriptor().stream_id;
        result.manifest.images.insert(
            id,
            StreamImageRef {
                prefix: Some(prefix),
                file: image_name(id, 1),
                len: bytes.len() as u64,
                crc64: crc64(&bytes),
            },
        );
        result.images.insert(id, bytes);
    }
    result
}

#[tokio::test]
async fn logged_bundle_preserves_exact_cross_stream_dependency_history() {
    let bundle = logged_bundle(false).await;
    let path = dir();
    bundle.publish(&path, &adapters()).await.unwrap();
    let reopened = GroupSnapshotBundle::read(&path, identity(), &adapters())
        .await
        .unwrap();
    let decoded = reopened.decode_logged_images(&adapters()).unwrap();
    assert_eq!(decoded.len(), 2);
    for (id, image) in decoded {
        let descriptor = &reopened.manifest.state.streams()[&id].descriptor;
        let target = DurableStreamLog::open(dir(), identity(), descriptor.clone())
            .await
            .unwrap();
        target.restore_snapshot(image.log()).await.unwrap();
        let request_id = if id == StreamId(1) { 10 } else { 11 };
        assert_eq!(
            target.request_progress(request_id).await.unwrap().as_ref(),
            image.log().prefix()
        );
        assert_eq!(image.business(), b"business");
    }
}

#[tokio::test]
async fn correct_checksums_do_not_hide_missing_dependency_proof_or_wrong_codec() {
    let bundle = logged_bundle(true).await;
    bundle.manifest.validate(identity(), &adapters()).unwrap();
    assert!(bundle.decode_logged_images(&adapters()).is_err());
    let mut bundle = logged_bundle(false).await;
    let id = StreamId(2);
    bundle.images.insert(id, b"not a logged image".to_vec());
    let image = bundle.manifest.images.get_mut(&id).unwrap();
    image.len = bundle.images[&id].len() as u64;
    image.crc64 = crc64(&bundle.images[&id]);
    assert!(bundle.decode_logged_images(&adapters()).is_err());
}
