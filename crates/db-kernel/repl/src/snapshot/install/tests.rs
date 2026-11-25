#![allow(clippy::unwrap_used)]
use super::*;

pub(super) async fn source() -> (String, ReplSnapshotManifest) {
    let root = mudu_sys::env_var::temp_dir()
        .join(format!(
            "snapshot-install-journal-{}",
            mudu_utils::oid::gen_oid()
        ))
        .to_string_lossy()
        .into_owned();
    let dir = snapshot_version_dir(&root, 0, 10);
    mudu_sys::fs::sync::create_dir_all(&dir).unwrap();
    let kv = rmp_serde::to_vec(&Vec::<KvDumpEntry>::new()).unwrap();
    mudu_sys::fs::sync::write(dir.join(KV_FILE_NAME), &kv).unwrap();
    let image = super::super::recovery::RecoveryImage {
        version: 1,
        worker_index: 0,
        wal_resume: 11,
        watermarks: Default::default(),
        participants: Default::default(),
    };
    let bytes = rmp_serde::to_vec(&image).unwrap();
    mudu_sys::fs::sync::write(dir.join("recovery.mp"), &bytes).unwrap();
    let mut manifest = ReplSnapshotManifest {
        version: 2,
        worker_index: 0,
        committed_lsn: 10,
        seg_hash: [0; 32],
        raft_term: 0,
        raft_index: 0,
        taken_at_ms: 0,
        files: vec![ReplSnapshotFileEntry {
            path: KV_FILE_NAME.into(),
            len: kv.len() as u64,
            crc64: crc64(&kv),
        }],
    };
    manifest.files.push(ReplSnapshotFileEntry {
        path: "recovery.mp".into(),
        len: bytes.len() as u64,
        crc64: crc64(&bytes),
    });
    seal_snapshot(&dir, &manifest).await.unwrap();
    (root, manifest)
}

#[test]
fn abandoned_install_survives_reopen_and_pins_source_versions() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (root, manifest) = source().await;
        ensure_no_pending_installs(&root).unwrap();
        let journal = InstallJournal::begin(&root, 42, &[0], &manifest)
            .await
            .unwrap();
        let intent: Intent =
            decode_intent(&mudu_sys::fs::sync::read(&journal.path).unwrap()).unwrap();
        assert_eq!(intent.worker_id, 42);
        assert_eq!(intent.manifest, manifest);
        drop(journal); // process loss/cancellation: there is no Drop cleanup
        assert!(ensure_no_pending_installs(&root).is_err());
        assert!(InstallJournal::begin(&root, 42, &[0], &manifest)
            .await
            .is_err());
        let worker_dir = snapshot_worker_dir(&root, 0);
        let old = snapshot_version_dir(&root, 0, 10);
        let new = snapshot_version_dir(&root, 0, 20);
        mudu_sys::fs::sync::create_dir_all(&new).unwrap();
        reclaim_old_versions(&worker_dir, &new).unwrap();
        assert!(mudu_sys::fs::sync::sync_path_exists(old));
        assert!(ensure_no_pending_installs(&root).is_err());
    })
    .unwrap();
}

#[test]
fn completed_install_clears_intent_and_pins_recovery_base() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (root, manifest) = source().await;
        InstallJournal::begin(&root, 42, &[0], &manifest)
            .await
            .unwrap()
            .complete()
            .await
            .unwrap();
        ensure_no_pending_installs(&root).unwrap();
        let worker_dir = snapshot_worker_dir(&root, 0);
        let old = snapshot_version_dir(&root, 0, 10);
        let new = snapshot_version_dir(&root, 0, 20);
        mudu_sys::fs::sync::create_dir_all(&new).unwrap();
        let unrelated = worker_dir.join("recovery-note");
        mudu_sys::fs::sync::write(&unrelated, b"retain").unwrap();
        reclaim_old_versions(&worker_dir, &new).unwrap();
        assert!(mudu_sys::fs::sync::sync_path_exists(old));
        assert!(mudu_sys::fs::sync::sync_path_exists(unrelated));
    })
    .unwrap();
}

#[test]
fn changed_intent_cannot_be_completed_or_ignored_on_startup() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (root, manifest) = source().await;
        let journal = InstallJournal::begin(&root, 42, &[0], &manifest)
            .await
            .unwrap();
        mudu_sys::fs::sync::write(&journal.path, b"broken").unwrap();
        assert!(journal.complete().await.is_err());
        assert!(ensure_no_pending_installs(&root).is_err());
        // Check all worker directories, independently of configured count.
        let removed = snapshot_worker_dir(&root, 99);
        mudu_sys::fs::sync::create_dir_all(&removed).unwrap();
        mudu_sys::fs::sync::sync_remove_file(snapshot_worker_dir(&root, 0).join(PENDING)).unwrap();
        mudu_sys::fs::sync::write(removed.join(PENDING), b"").unwrap();
        assert!(ensure_no_pending_installs(&root).is_err());
    })
    .unwrap();
}

#[test]
fn corrupt_source_is_rejected_before_creating_install_intent() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (root, manifest) = source().await;
        let dir = snapshot_version_dir(&root, 0, 10);
        mudu_sys::fs::sync::write(dir.join(KV_FILE_NAME), b"wrong").unwrap();
        assert!(InstallJournal::begin(&root, 42, &[0], &manifest)
            .await
            .is_err());
        ensure_no_pending_installs(&root).unwrap();
    })
    .unwrap();
}
