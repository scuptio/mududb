#![allow(clippy::unwrap_used)]
use super::*;
use crate::log_stream::tests::{adapters, apply, change, commit, descriptor, identity, progress};
use mudu_raft::{stream::RegistryOp, ConfValue, NodeInfo};

pub(super) fn bundle() -> GroupSnapshotBundle {
    let mut state = GroupState::new(identity()).unwrap();
    for id in [1, 2] {
        let entry = change(&state, id as u128, RegistryOp::Register(descriptor(id)));
        apply(&mut state, &entry).unwrap();
    }
    let entry = commit(&state, vec![progress(1, 0)]);
    apply(&mut state, &entry).unwrap();
    let conf = ConfNodeValue::new(
        ConfValue::new(
            1,
            10,
            vec![NodeInfo {
                node_id: 1,
                can_vote: true,
            }],
        ),
        1,
        1,
        1,
    );
    let mut manifest = GroupSnapshotManifest {
        version: 1,
        state,
        committed_conf: conf.clone(),
        incoming_conf: conf,
        images: BTreeMap::new(),
    };
    let mut images = BTreeMap::new();
    for id in [StreamId(1), StreamId(2)] {
        let bytes = vec![id.0 as u8; 10];
        manifest.images.insert(
            id,
            StreamImageRef {
                prefix: manifest.state.committed().get(&id).cloned(),
                file: image_name(id, 1),
                len: 10,
                crc64: crc64(&bytes),
            },
        );
        images.insert(id, bytes);
    }
    GroupSnapshotBundle { manifest, images }
}
pub(super) fn dir() -> std::path::PathBuf {
    mudu_sys::env_var::temp_dir().join(format!("group-snapshot-{}", mudu_sys::random::uuid_v4()))
}

#[test]
fn complete_bundle_reopens_and_cannot_be_replaced() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = dir();
        let mut bundle = bundle();
        bundle.publish(&dir, &adapters()).await.unwrap();
        bundle.publish(&dir, &adapters()).await.unwrap();
        let loaded = GroupSnapshotBundle::read(&dir, identity(), &adapters())
            .await
            .unwrap();
        assert_eq!(loaded.images, bundle.images);
        assert_eq!(
            loaded.manifest.state.committed(),
            bundle.manifest.state.committed()
        );
        assert!(loaded.manifest.images[&StreamId(2)].prefix.is_none());
        bundle.images.get_mut(&StreamId(1)).unwrap()[0] ^= 1;
        bundle.manifest.images.get_mut(&StreamId(1)).unwrap().crc64 =
            crc64(&bundle.images[&StreamId(1)]);
        assert!(bundle.publish(&dir, &adapters()).await.is_err());
        assert_eq!(
            GroupSnapshotBundle::read(&dir, identity(), &adapters())
                .await
                .unwrap()
                .images,
            loaded.images
        );
    })
    .unwrap();
}

#[test]
fn corrupt_missing_foreign_or_unknown_images_fail_closed() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = dir();
        let bundle = bundle();
        bundle.publish(&dir, &adapters()).await.unwrap();
        let mut foreign = identity();
        foreign.generation += 1;
        assert!(GroupSnapshotBundle::read(&dir, foreign, &adapters())
            .await
            .is_err());
        assert!(
            GroupSnapshotBundle::read(&dir, identity(), &AdapterCapabilities::default())
                .await
                .is_err()
        );
        let fs = mudu_sys::fs::tokio_fs();
        let path = dir.join(&bundle.manifest.images[&StreamId(1)].file);
        crate::durable_io::publish(fs.as_ref(), &path, &[9; 10])
            .await
            .unwrap();
        assert!(GroupSnapshotBundle::read(&dir, identity(), &adapters())
            .await
            .is_err());
        fs.remove_file_if_exists(&path).await.unwrap();
        assert!(GroupSnapshotBundle::read(&dir, identity(), &adapters())
            .await
            .is_err());
    })
    .unwrap();
}

#[test]
fn manifest_rejects_wrong_coverage_cut_paths_membership_and_budgets() {
    let base = bundle().manifest;
    base.validate(identity(), &adapters()).unwrap();
    for case in 0..9 {
        let mut bad = base.clone();
        match case {
            0 => {
                bad.images.remove(&StreamId(2));
            }
            1 => {
                bad.images.get_mut(&StreamId(1)).unwrap().prefix = Some(progress(1, 1));
            }
            2 => {
                bad.images.get_mut(&StreamId(2)).unwrap().file = "../escape".into();
            }
            3 => {
                bad.incoming_conf.node.conf_version.index = 99;
            }
            4 => {
                bad.incoming_conf.node.nid_vote.push(2);
            }
            5 => {
                bad.incoming_conf.node.conf_version.index += 1;
            }
            6 => {
                bad.images.get_mut(&StreamId(1)).unwrap().len = MAX_IMAGE_BYTES + 1;
            }
            7 => {
                bad.version += 1;
            }
            _ => {
                bad.images
                    .insert(StreamId(3), bad.images[&StreamId(2)].clone());
            }
        }
        assert!(
            bad.validate(identity(), &adapters()).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn partial_unsealed_directory_is_not_a_recovery_base() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = dir();
        let bundle = bundle();
        let fs = mudu_sys::fs::tokio_fs();
        crate::durable_io::ensure_dir(fs.as_ref(), &dir)
            .await
            .unwrap();
        crate::durable_io::publish(
            fs.as_ref(),
            &dir.join(image_name(StreamId(1), 1)),
            &bundle.images[&StreamId(1)],
        )
        .await
        .unwrap();
        assert!(GroupSnapshotBundle::read(&dir, identity(), &adapters())
            .await
            .is_err());
        bundle.publish(&dir, &adapters()).await.unwrap();
        assert!(GroupSnapshotBundle::read(&dir, identity(), &adapters())
            .await
            .is_ok());
    })
    .unwrap();
}

#[test]
fn manifest_corruption_and_mismatched_payload_never_publish_success() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = dir();
        let mut image = bundle();
        image.images.get_mut(&StreamId(1)).unwrap()[0] ^= 1;
        assert!(image.publish(&dir, &adapters()).await.is_err());
        assert!(!mudu_sys::fs::tokio_fs()
            .path_exists(&dir.join(MANIFEST))
            .await
            .unwrap());
        let image = bundle();
        image.publish(&dir, &adapters()).await.unwrap();
        let fs = mudu_sys::fs::tokio_fs();
        let path = dir.join(MANIFEST);
        let mut bytes = fs.read_all(&path).await.unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        crate::durable_io::publish(fs.as_ref(), &path, &bytes)
            .await
            .unwrap();
        assert!(GroupSnapshotBundle::read(&dir, identity(), &adapters())
            .await
            .is_err());
        assert!(image.publish(&dir, &adapters()).await.is_err());
    })
    .unwrap();
}

#[test]
fn joint_membership_and_retired_empty_streams_keep_exact_coverage() {
    let mut manifest = bundle().manifest;
    manifest.incoming_conf = ConfNodeValue::new(
        ConfValue::new(
            1,
            10,
            vec![
                NodeInfo {
                    node_id: 1,
                    can_vote: true,
                },
                NodeInfo {
                    node_id: 2,
                    can_vote: true,
                },
            ],
        ),
        1,
        2,
        manifest.state.frontier().index,
    );
    manifest.validate(identity(), &adapters()).unwrap();
    let deactivate = change(
        &manifest.state,
        3,
        RegistryOp::Deactivate {
            stream_id: StreamId(2),
            stream_generation: 1,
        },
    );
    apply(&mut manifest.state, &deactivate).unwrap();
    // A draining stream still needs an image, even when it is empty.
    manifest.validate(identity(), &adapters()).unwrap();
    let remove = change(
        &manifest.state,
        4,
        RegistryOp::Remove {
            stream_id: StreamId(2),
            stream_generation: 1,
        },
    );
    apply(&mut manifest.state, &remove).unwrap();
    assert!(manifest.validate(identity(), &adapters()).is_err());
    manifest.images.remove(&StreamId(2));
    manifest.validate(identity(), &adapters()).unwrap();
}
