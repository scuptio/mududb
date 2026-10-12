#![allow(clippy::unwrap_used)]
#![allow(unused_imports, dead_code)]
//! Directory-object tests for [`super::super::fs_service::FsService`].
//!
//! Fixtures ([`super::MemFs`], [`super::MockFsObjectStore`], the [`super::Fixture`]
//! builder) live in the parent module; this child only carries the tests.

use super::*;

#[test]
fn directory_object_supports_entries_readdir_and_stat() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_000Bu128;
        stage_pending(&f, oid, DIR_FS_ID, FsTypeKind::Directory);

        for (entry, payload) in [
            ("a/b", b"B".as_slice()),
            ("a/c", b"CC".as_slice()),
            ("x", b"x".as_slice()),
        ] {
            let fd = f
                .service
                .fs_open(f.session_id, oid, entry, O_WRONLY)
                .await
                .unwrap();
            f.service.fs_write(f.session_id, fd, payload).await.unwrap();
            f.service.fs_close(f.session_id, fd).await.unwrap();
        }
        for (entry, payload) in [
            ("a/b", b"B".as_slice()),
            ("a/c", b"CC".as_slice()),
            ("x", b"x".as_slice()),
        ] {
            assert_eq!(
                f.fs.read_file(&f.content_path(DIR_FS_ID, oid, 1, entry))
                    .unwrap(),
                payload.to_vec()
            );
        }
        // Flat layout: the generation has no `{oidhex}.1` host path of its
        // own; entries are prefix-named siblings directly under the fs root
        // and `{oidhex}.1.a` is a real directory created on demand.
        let err =
            f.fs.metadata_len(&f.content_path(DIR_FS_ID, oid, 1, ""))
                .await
                .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);
        assert_eq!(
            f.fs.metadata_len(&f.content_path(DIR_FS_ID, oid, 1, "a"))
                .await
                .unwrap(),
            0
        );

        make_sealed(&f, oid, DIR_FS_ID, FsTypeKind::Directory, 1, 0);

        // The directory root cannot be opened as a file.
        let err = f
            .service
            .fs_open(f.session_id, oid, "", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IsADirectory);
        // Neither can a directory entry.
        let err = f
            .service
            .fs_open(f.session_id, oid, "a", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IsADirectory);

        let entries = f.service.fs_readdir(f.session_id, oid, "").await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "a");
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].length, 0);
        assert_eq!(entries[1].name, "x");
        assert!(!entries[1].is_dir);
        assert_eq!(entries[1].length, 1);
        let entries = f.service.fs_readdir(f.session_id, oid, "a").await.unwrap();
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["b", "c"]);
        assert!(entries.iter().all(|entry| !entry.is_dir));
        assert_eq!(entries[0].length, 1);
        assert_eq!(entries[1].length, 2);

        let stat = f.service.fs_stat(f.session_id, oid, "a").await.unwrap();
        assert_eq!(stat.length, 0);
        let stat = f.service.fs_stat(f.session_id, oid, "x").await.unwrap();
        assert_eq!(stat.length, 1);
        assert_eq!(stat.entry, "x");

        // Entries of the sealed generation open read-only.
        let fd = f
            .service
            .fs_open(f.session_id, oid, "a/c", O_RDONLY)
            .await
            .unwrap();
        assert_eq!(
            f.service.fs_read(f.session_id, fd, 8).await.unwrap(),
            b"CC".to_vec()
        );
        f.service.fs_close(f.session_id, fd).await.unwrap();
    });
}

#[test]
fn readdir_of_empty_directory_object_is_empty() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0011u128;
        make_sealed(&f, oid, DIR_FS_ID, FsTypeKind::Directory, 1, 0);

        // Nothing was ever written, so the fs root does not even exist; the
        // virtual object root still lists and stats as an empty directory.
        let entries = f.service.fs_readdir(f.session_id, oid, "").await.unwrap();
        assert!(entries.is_empty());
        let stat = f.service.fs_stat(f.session_id, oid, "").await.unwrap();
        assert_eq!(stat.entry, "");
        assert_eq!(stat.length, 0);
    });
}

#[test]
fn readdir_root_matches_only_the_owning_generation_prefix() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0012u128;
        stage_pending(&f, oid, DIR_FS_ID, FsTypeKind::Directory);
        let fd = f
            .service
            .fs_open(f.session_id, oid, "x", O_WRONLY)
            .await
            .unwrap();
        f.service.fs_write(f.session_id, fd, b"x").await.unwrap();
        f.service.fs_close(f.session_id, fd).await.unwrap();
        make_sealed(&f, oid, DIR_FS_ID, FsTypeKind::Directory, 1, 1);

        // Foreign names under the same fs root never leak into the listing:
        // the bare generation name, another generation (dot boundary), a name
        // continuing past the generation without a dot, a foreign oid, and
        // unrelated junk.
        let root = fs_storage_root(&f.data_dir, DIR_FS_ID);
        let other_oid = 0xF500_0000_0000_0013u128;
        for name in [
            format!("{oid:032x}.1"),
            format!("{oid:032x}.12.y"),
            format!("{oid:032x}.1x.z"),
            format!("{other_oid:032x}.1.q"),
            "unrelated".to_string(),
        ] {
            f.fs.open(&root.join(name), FileOptions::read_write_create())
                .await
                .unwrap();
        }

        let entries = f.service.fs_readdir(f.session_id, oid, "").await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "x");
        assert!(!entries[0].is_dir);
        assert_eq!(entries[0].length, 1);
    });
}

#[test]
fn file_object_rejects_paths_and_readdir() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_000Cu128;
        make_sealed(&f, oid, FILE_FS_ID, FsTypeKind::File, 2, 4);

        let err = f
            .service
            .fs_open(f.session_id, oid, "x", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotADirectory);
        let err = f.service.fs_stat(f.session_id, oid, "x").await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotADirectory);
        let err = f
            .service
            .fs_readdir(f.session_id, oid, "")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotADirectory);
    });
}

#[test]
fn path_normalization_enforces_the_object_sandbox() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_000Du128;
        make_sealed(&f, oid, DIR_FS_ID, FsTypeKind::Directory, 1, 0);

        let err = f
            .service
            .fs_open(f.session_id, oid, "../x", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::PermissionDenied);
        let err = f
            .service
            .fs_open(f.session_id, oid, "a/../../x", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::PermissionDenied);
        let err = f
            .service
            .fs_open(f.session_id, oid, "/abs", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidFilename);
        let err = f
            .service
            .fs_open(f.session_id, oid, "a\0b", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidFilename);
        let err = f
            .service
            .fs_open(f.session_id, oid, "a//b", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidFilename);

        // `.` components are normalized away; `.` alone addresses the root.
        let stat = f.service.fs_stat(f.session_id, oid, ".").await.unwrap();
        assert_eq!(stat.entry, "");
        assert_eq!(stat.length, 0);
    });
}
