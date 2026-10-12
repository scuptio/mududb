#![allow(clippy::unwrap_used)]
#![allow(unused_imports, dead_code)]
//! Cursor, transaction, and session tests for
//! [`super::super::fs_service::FsService`].
//!
//! Fixtures ([`super::MemFs`], [`super::MockFsObjectStore`], the [`super::Fixture`]
//! builder) live in the parent module; this child only carries the tests.

use super::*;

#[test]
fn lseek_validates_whence_and_resulting_cursor() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_000Eu128;
        stage_pending(&f, oid, FILE_FS_ID, FsTypeKind::File);
        let fd = f
            .service
            .fs_open(f.session_id, oid, "", O_WRONLY)
            .await
            .unwrap();
        f.service
            .fs_write(f.session_id, fd, b"0123456789")
            .await
            .unwrap();

        assert_eq!(f.service.fs_lseek(f.session_id, fd, -4, 2).unwrap(), 6);
        assert_eq!(f.service.fs_lseek(f.session_id, fd, 2, 1).unwrap(), 8);
        // Seeking past EOF is allowed.
        assert_eq!(f.service.fs_lseek(f.session_id, fd, 100, 0).unwrap(), 100);
        let err = f.service.fs_lseek(f.session_id, fd, -1, 0).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        let err = f.service.fs_lseek(f.session_id, fd, -101, 1).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        let err = f.service.fs_lseek(f.session_id, fd, 0, 7).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    });
}

#[test]
fn fs_syscalls_require_an_active_transaction() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_000Fu128;
        stage_pending(&f, oid, FILE_FS_ID, FsTypeKind::File);

        // Session without a transaction.
        let plain = f.sessions.create_session(1).unwrap();
        let err = f
            .service
            .fs_open(plain, oid, "", O_WRONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidState);

        // Unknown session.
        let err = f
            .service
            .fs_open(0xDEAD_BEEFu128, oid, "", O_WRONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);

        // Read-only helpers require the transaction too.
        let err = f.service.fs_stat(plain, oid, "").await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidState);
    });
}

#[test]
fn drop_session_reclaims_session_fds() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0010u128;
        stage_pending(&f, oid, FILE_FS_ID, FsTypeKind::File);
        let fd = f
            .service
            .fs_open(f.session_id, oid, "", O_WRONLY)
            .await
            .unwrap();

        f.service.drop_session(f.session_id);
        let err = f
            .service
            .fs_write(f.session_id, fd, b"x")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);

        // Dropping twice is a no-op.
        f.service.drop_session(f.session_id);
    });
}

#[test]
fn stat_of_unknown_object_is_not_found() {
    block_on(async {
        let f = new_fixture();
        let err = f
            .service
            .fs_stat(f.session_id, 0xF5BBu128, "")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);
        let err = f
            .service
            .fs_readdir(f.session_id, 0xF5BBu128, "")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);
    });
}
