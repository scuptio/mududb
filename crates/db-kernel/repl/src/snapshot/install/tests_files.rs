#![allow(clippy::unwrap_used)]
use super::*;

fn live(root: &str, path: &str, bytes: &[u8]) {
    let path = Path::new(root).join(path);
    mudu_sys::fs::sync::create_dir_all(path.parent().unwrap()).unwrap();
    mudu_sys::fs::sync::write(path, bytes).unwrap();
}

async fn with_relation(root: &str, manifest: &mut ReplSnapshotManifest, path: &str) {
    let dir = snapshot_version_dir(root, 0, manifest.committed_lsn);
    let bytes = b"new relation";
    live(dir.to_str().unwrap(), path, bytes);
    manifest.files.push(ReplSnapshotFileEntry {
        path: path.into(),
        len: bytes.len() as u64,
        crc64: crc64(bytes),
    });
    // Build a new sealed fixture; production never changes a sealed manifest.
    mudu_sys::fs::sync::sync_remove_file(dir.join(MANIFEST_FILE_NAME)).unwrap();
    seal_snapshot(&dir, manifest).await.unwrap();
}

#[test]
fn exact_install_removes_only_owned_files_and_invalidates_indexes() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (root, mut manifest) = tests::source().await;
        with_relation(&root, &mut manifest, "relation/7.1.dat").await;
        for name in [
            "relation/7.1.dat",
            "relation/7.2.dat",
            "relation/8.1.dat",
            "relation/7.1.dat.index-manifest",
            "relation/8.1.dat.index-manifest",
            "meta/relation/old.dat",
            "meta/relation/old.dat.index-manifest",
        ] {
            live(&root, name, b"old");
        }
        let journal = InstallJournal::begin(&root, 42, &[7], &manifest)
            .await
            .unwrap();
        journal.install_files().await.unwrap();
        journal.install_files().await.unwrap(); // repeated file phase is idempotent
        assert_eq!(
            mudu_sys::fs::sync::read(Path::new(&root).join("relation/7.1.dat")).unwrap(),
            b"new relation"
        );
        for name in [
            "relation/7.2.dat",
            "relation/7.1.dat.index-manifest",
            "meta/relation/old.dat",
            "meta/relation/old.dat.index-manifest",
        ] {
            assert!(!mudu_sys::fs::sync::sync_path_exists(
                Path::new(&root).join(name)
            ));
        }
        for name in ["relation/8.1.dat", "relation/8.1.dat.index-manifest"] {
            assert_eq!(
                mudu_sys::fs::sync::read(Path::new(&root).join(name)).unwrap(),
                b"old"
            );
        }
        assert!(ensure_no_pending_installs(&root).is_err());
        journal.complete().await.unwrap();
        ensure_no_pending_installs(&root).unwrap();
    })
    .unwrap();
}

#[test]
fn foreign_source_and_new_live_files_are_rejected_before_mutation() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (root, mut manifest) = tests::source().await;
        with_relation(&root, &mut manifest, "relation/8.1.dat").await;
        assert!(InstallJournal::begin(&root, 42, &[7], &manifest)
            .await
            .is_err());
        ensure_no_pending_installs(&root).unwrap();
        let journal = InstallJournal::begin(&root, 42, &[8], &manifest)
            .await
            .unwrap();
        live(&root, "relation/8.2.dat", b"unexpected");
        live(&root, "relation/8.1.dat.index-manifest", b"retain");
        assert!(journal.install_files().await.is_err());
        assert_eq!(
            mudu_sys::fs::sync::read(Path::new(&root).join("relation/8.1.dat.index-manifest"))
                .unwrap(),
            b"retain"
        );
        assert!(!mudu_sys::fs::sync::sync_path_exists(
            Path::new(&root).join("relation/8.1.dat")
        ));
    })
    .unwrap();
}

#[test]
fn changed_source_is_rejected_before_index_invalidation() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (root, manifest) = tests::source().await;
        live(&root, "relation/0.1.dat.index-manifest", b"retain");
        let journal = InstallJournal::begin(&root, 42, &[0], &manifest)
            .await
            .unwrap();
        let source = snapshot_version_dir(&root, 0, 10);
        mudu_sys::fs::sync::write(source.join(KV_FILE_NAME), b"corrupt").unwrap();
        assert!(journal.install_files().await.is_err());
        assert_eq!(
            mudu_sys::fs::sync::read(Path::new(&root).join("relation/0.1.dat.index-manifest"))
                .unwrap(),
            b"retain"
        );
    })
    .unwrap();
}

#[test]
fn conflicting_recovery_base_is_rejected_before_intent() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (root, manifest) = tests::source().await;
        super::super::base::publish(&root, 99, &manifest)
            .await
            .unwrap();
        assert!(InstallJournal::begin(&root, 42, &[0], &manifest)
            .await
            .is_err());
        ensure_no_pending_installs(&root).unwrap();
    })
    .unwrap();
}
