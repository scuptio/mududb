#![allow(clippy::unwrap_used)]
#![allow(unused_imports, dead_code)]
//! Fd lifecycle and open-flag tests for [`super::super::fs_service::FsService`].
//!
//! Fixtures ([`super::MemFs`], [`super::MockFsObjectStore`], the [`super::Fixture`]
//! builder) live in the parent module; this child only carries the tests.

use super::*;

#[test]
fn write_close_seals_row_then_read_open_reads_content() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0001u128;
        stage_pending(&f, oid, FILE_FS_ID, FsTypeKind::File);

        let fd = f
            .service
            .fs_open(f.session_id, oid, "", O_WRONLY)
            .await
            .unwrap();
        assert_eq!(fd, 0);
        // The write fd anchors the new private generation.
        let stat = f.service.fs_fstat(f.session_id, fd).unwrap();
        assert_eq!(stat.oid.to_oid(), oid);
        assert_eq!(stat.generation, 1);
        assert_eq!(stat.entry, "");
        assert_eq!(stat.length, 0);
        assert_eq!(stat.state, FS_OBJECT_STATE_PENDING);

        assert_eq!(
            f.service
                .fs_write(f.session_id, fd, b"hello")
                .await
                .unwrap(),
            5
        );
        // Positioned write past EOF leaves a sparse hole.
        f.service
            .fs_pwrite(f.session_id, fd, 10, b"z")
            .await
            .unwrap();
        let stat = f.service.fs_fstat(f.session_id, fd).unwrap();
        assert_eq!(stat.length, 11);
        f.service.fs_fsync(f.session_id, fd).await.unwrap();
        f.service.fs_close(f.session_id, fd).await.unwrap();

        // Closing sealed the row into the transaction with the final length.
        let staged = f.tx.staged_fs_row(0, oid).unwrap().unwrap();
        let row = decode_fs_object_row(&staged).unwrap();
        assert_eq!(row, sealed_row(FILE_FS_ID, FsTypeKind::File, 1, 11));
        // Content landed in the private generation file, fsynced on close.
        let content_path = f.content_path(FILE_FS_ID, oid, 1, "");
        assert_eq!(
            f.fs.read_file(&content_path).unwrap(),
            b"hello\0\0\0\0\0z".to_vec()
        );
        assert!(f.fs.fsynced().contains(&content_path));

        // Reopen the sealed object for reading; the fd value is reused.
        make_sealed(&f, oid, FILE_FS_ID, FsTypeKind::File, 1, 11);
        let fd = f
            .service
            .fs_open(f.session_id, oid, "", O_RDONLY)
            .await
            .unwrap();
        assert_eq!(fd, 0);

        assert_eq!(
            f.service.fs_read(f.session_id, fd, 4).await.unwrap(),
            b"hell".to_vec()
        );
        // pread does not move the cursor.
        assert_eq!(
            f.service.fs_pread(f.session_id, fd, 10, 1).await.unwrap(),
            b"z".to_vec()
        );
        assert_eq!(
            f.service.fs_read(f.session_id, fd, 1).await.unwrap(),
            b"o".to_vec()
        );
        // Short read at the end of the file, then EOF as an empty buffer.
        assert_eq!(f.service.fs_lseek(f.session_id, fd, 9, 0).unwrap(), 9);
        assert_eq!(
            f.service.fs_read(f.session_id, fd, 8).await.unwrap(),
            b"\0z".to_vec()
        );
        assert_eq!(
            f.service.fs_read(f.session_id, fd, 1).await.unwrap(),
            Vec::<u8>::new()
        );
        assert_eq!(
            f.service.fs_pread(f.session_id, fd, 11, 3).await.unwrap(),
            Vec::<u8>::new()
        );

        let stat = f.service.fs_fstat(f.session_id, fd).unwrap();
        assert_eq!(stat.state, FS_OBJECT_STATE_SEALED);
        assert_eq!(stat.generation, 1);
        assert_eq!(stat.length, 11);

        let stat = f.service.fs_stat(f.session_id, oid, "").await.unwrap();
        assert_eq!(stat.length, 11);
        assert_eq!(stat.state, FS_OBJECT_STATE_SEALED);
        assert_eq!(stat.generation, 1);

        f.service.fs_close(f.session_id, fd).await.unwrap();
    });
}

#[test]
fn rdwr_fd_reads_and_writes_own_generation() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0002u128;
        stage_pending(&f, oid, FILE_FS_ID, FsTypeKind::File);

        let fd = f
            .service
            .fs_open(f.session_id, oid, "", O_RDWR)
            .await
            .unwrap();
        f.service.fs_write(f.session_id, fd, b"abc").await.unwrap();
        assert_eq!(f.service.fs_lseek(f.session_id, fd, 0, 0).unwrap(), 0);
        assert_eq!(
            f.service.fs_read(f.session_id, fd, 3).await.unwrap(),
            b"abc".to_vec()
        );
        f.service.fs_close(f.session_id, fd).await.unwrap();
        let staged = f.tx.staged_fs_row(0, oid).unwrap().unwrap();
        assert_eq!(decode_fs_object_row(&staged).unwrap().length, 3);
    });
}

#[test]
fn access_mode_is_enforced_on_fds() {
    block_on(async {
        let f = new_fixture();
        let write_oid = 0xF500_0000_0000_0003u128;
        stage_pending(&f, write_oid, FILE_FS_ID, FsTypeKind::File);
        let write_fd = f
            .service
            .fs_open(f.session_id, write_oid, "", O_WRONLY)
            .await
            .unwrap();
        let err = f
            .service
            .fs_read(f.session_id, write_fd, 1)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);

        let read_oid = 0xF500_0000_0000_0004u128;
        stage_pending(&f, read_oid, FILE_FS_ID, FsTypeKind::File);
        let fd = f
            .service
            .fs_open(f.session_id, read_oid, "", O_WRONLY)
            .await
            .unwrap();
        f.service.fs_write(f.session_id, fd, b"xy").await.unwrap();
        f.service.fs_close(f.session_id, fd).await.unwrap();
        make_sealed(&f, read_oid, FILE_FS_ID, FsTypeKind::File, 1, 2);

        let read_fd = f
            .service
            .fs_open(f.session_id, read_oid, "", O_RDONLY)
            .await
            .unwrap();
        let err = f
            .service
            .fs_write(f.session_id, read_fd, b"n")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
        let err = f
            .service
            .fs_pwrite(f.session_id, read_fd, 0, b"n")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
        let err = f.service.fs_fsync(f.session_id, read_fd).await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    });
}

#[test]
fn closed_fd_access_is_bad_file_descriptor() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0005u128;
        stage_pending(&f, oid, FILE_FS_ID, FsTypeKind::File);
        let fd = f
            .service
            .fs_open(f.session_id, oid, "", O_WRONLY)
            .await
            .unwrap();
        f.service.fs_close(f.session_id, fd).await.unwrap();

        let err = f.service.fs_close(f.session_id, fd).await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
        let err = f.service.fs_read(f.session_id, fd, 1).await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
        let err = f.service.fs_lseek(f.session_id, fd, 0, 0).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
        let err = f.service.fs_fstat(f.session_id, fd).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
    });
}

#[test]
fn unsupported_open_flags_are_invalid_argument() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0006u128;
        make_sealed(&f, oid, FILE_FS_ID, FsTypeKind::File, 1, 0);
        for flags in [O_CREAT, O_EXCL, O_TRUNC, O_APPEND, 3] {
            let err = f
                .service
                .fs_open(f.session_id, oid, "", flags)
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::InvalidArgument, "flags {flags:#o}");
        }
    });
}

#[test]
fn io_payload_limit_is_enforced() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0007u128;
        stage_pending(&f, oid, FILE_FS_ID, FsTypeKind::File);
        let fd = f
            .service
            .fs_open(f.session_id, oid, "", O_RDWR)
            .await
            .unwrap();
        let err = f
            .service
            .fs_read(f.session_id, fd, FS_IO_MAX_BYTES + 1)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        let err = f
            .service
            .fs_pread(f.session_id, fd, 0, FS_IO_MAX_BYTES + 1)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        let big = vec![0u8; FS_IO_MAX_BYTES as usize + 1];
        let err = f
            .service
            .fs_write(f.session_id, fd, &big)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        let err = f
            .service
            .fs_pwrite(f.session_id, fd, 0, &big)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    });
}

#[test]
fn open_requires_sealed_row_for_read_and_owned_pending_row_for_write() {
    block_on(async {
        let f = new_fixture();

        // Unknown oid.
        let err = f
            .service
            .fs_open(f.session_id, 0xF5AAu128, "", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);

        // A PENDING object cannot be read even by its owning transaction.
        let pending_oid = 0xF500_0000_0000_0008u128;
        stage_pending(&f, pending_oid, FILE_FS_ID, FsTypeKind::File);
        let err = f
            .service
            .fs_open(f.session_id, pending_oid, "", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);

        // A sealed object is immutable: write open is rejected.
        let sealed_oid = 0xF500_0000_0000_0009u128;
        make_sealed(&f, sealed_oid, FILE_FS_ID, FsTypeKind::File, 3, 5);
        let err = f
            .service
            .fs_open(f.session_id, sealed_oid, "", O_WRONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::PermissionDenied);

        // A PENDING row only visible from storage belongs to another
        // transaction: write open is rejected.
        let foreign_oid = 0xF500_0000_0000_000Au128;
        f.store.insert(
            foreign_oid,
            0,
            pending_row(FILE_FS_ID, FsTypeKind::File),
            false,
        );
        let err = f
            .service
            .fs_open(f.session_id, foreign_oid, "", O_WRONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::PermissionDenied);
    });
}
