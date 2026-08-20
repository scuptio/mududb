// Miri cannot execute FFI calls into SQLite (via rusqlite), so skip these
// integration tests under Miri. They are still exercised by normal `cargo test`.
#![cfg(all(not(target_arch = "wasm32"), feature = "standalone-adapter"))]

use mudu::error::ErrorCode;
use mudu_sys::sync::SMutexGuard;
use mudu_sys::time::system_time_now;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;
use sys_interface::{async_api, sync_api};

fn lock_tests() -> SMutexGuard<'static, ()> {
    mudu_adapter::config::test_lock()
        .lock()
        .expect("test lock poisoned")
}

fn temp_db_path(name: &str) -> PathBuf {
    let suffix = system_time_now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    mudu_sys::env_var::temp_dir().join(format!("sys_interface_fs_{name}_{suffix}.db"))
}

fn use_temp_db(name: &str) -> PathBuf {
    let db_path = temp_db_path(name);
    mudu_adapter::config::reset_db_path_override_for_test();
    mudu_adapter::syscall::set_db_path(&db_path);
    db_path
}

// libc open(2) flag values used by the fs syscall family.
const O_RDONLY: u32 = 0;
const O_WRONLY: u32 = 1;
const O_RDWR: u32 = 2;
const O_CREAT: u32 = 0o100;

// libc lseek(2) whence values.
const SEEK_SET: u32 = 0;
const SEEK_CUR: u32 = 1;
const SEEK_END: u32 = 2;

#[test]
#[cfg_attr(miri, ignore)]
fn sync_standalone_fs_file_roundtrip() {
    let _guard = lock_tests();
    use_temp_db("file_roundtrip");

    let session_id = sync_api::mudu_open().unwrap();
    let oid: u128 = 0xF500_0000_0000_0000_0000_0000_0000_0001;

    // Write-open creates the content file; sequential writes advance the cursor.
    let fd = sync_api::mudu_fs_open(session_id, oid, "", O_WRONLY).unwrap();
    assert_eq!(
        sync_api::mudu_fs_write(session_id, fd, b"hello ").unwrap(),
        6
    );
    assert_eq!(
        sync_api::mudu_fs_write(session_id, fd, b"world").unwrap(),
        5
    );

    // Positional write past EOF leaves a sparse hole and grows the length.
    sync_api::mudu_fs_pwrite(session_id, fd, 1024, b"sparse").unwrap();
    sync_api::mudu_fs_fsync(session_id, fd).unwrap();

    let stat = sync_api::mudu_fs_fstat(session_id, fd).unwrap();
    assert_eq!(stat.oid, oid);
    assert_eq!(stat.generation, 1);
    assert_eq!(stat.entry, "");
    assert_eq!(stat.length, 1030);
    assert_eq!(stat.state, 1);

    // lseek SET/CUR/END works on a write fd; reads require read access.
    assert_eq!(
        sync_api::mudu_fs_lseek(session_id, fd, 0, SEEK_SET).unwrap(),
        0
    );
    assert_eq!(
        sync_api::mudu_fs_lseek(session_id, fd, 11, SEEK_CUR).unwrap(),
        11
    );
    assert_eq!(
        sync_api::mudu_fs_lseek(session_id, fd, 0, SEEK_END).unwrap(),
        1030
    );
    sync_api::mudu_fs_close(session_id, fd).unwrap();

    // Read-open requires an existing file and anchors to the same content.
    let fd = sync_api::mudu_fs_open(session_id, oid, "", O_RDONLY).unwrap();
    assert_eq!(
        sync_api::mudu_fs_read(session_id, fd, 11).unwrap(),
        b"hello world"
    );
    // pread does not move the cursor; the sparse hole reads back as zeros.
    assert_eq!(
        sync_api::mudu_fs_pread(session_id, fd, 1024, 6).unwrap(),
        b"sparse"
    );
    let hole = sync_api::mudu_fs_pread(session_id, fd, 16, 4).unwrap();
    assert_eq!(hole, vec![0u8; 4]);
    // pread did not move the cursor; reads resume from it.
    assert_eq!(
        sync_api::mudu_fs_lseek(session_id, fd, 0, SEEK_CUR).unwrap(),
        11
    );
    assert_eq!(
        sync_api::mudu_fs_lseek(session_id, fd, 6, SEEK_SET).unwrap(),
        6
    );
    assert_eq!(sync_api::mudu_fs_read(session_id, fd, 5).unwrap(), b"world");
    // pread at/past EOF yields an empty buffer.
    assert!(
        sync_api::mudu_fs_pread(session_id, fd, 2048, 8)
            .unwrap()
            .is_empty()
    );
    // read clamps at EOF: short, then empty.
    assert_eq!(
        sync_api::mudu_fs_lseek(session_id, fd, 1024, SEEK_SET).unwrap(),
        1024
    );
    assert_eq!(
        sync_api::mudu_fs_read(session_id, fd, 64).unwrap(),
        b"sparse"
    );
    assert!(
        sync_api::mudu_fs_read(session_id, fd, 8)
            .unwrap()
            .is_empty()
    );

    let stat = sync_api::mudu_fs_fstat(session_id, fd).unwrap();
    assert_eq!(stat.length, 1030);

    // stat without an fd reports the same frame.
    let stat = sync_api::mudu_fs_stat(session_id, oid, "").unwrap();
    assert_eq!(stat.oid, oid);
    assert_eq!(stat.generation, 1);
    assert_eq!(stat.entry, "");
    assert_eq!(stat.length, 1030);
    assert_eq!(stat.state, 1);

    sync_api::mudu_fs_close(session_id, fd).unwrap();
    sync_api::mudu_close(session_id).unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_standalone_fs_readdir_lists_directory_entries() {
    let _guard = lock_tests();
    use_temp_db("readdir");

    let session_id = sync_api::mudu_open().unwrap();
    let oid: u128 = 0xF500_0000_0000_0000_0000_0000_0000_0002;

    for (entry, content) in [
        ("docs/a.txt", b"A".as_slice()),
        ("docs/b.txt", b"BB".as_slice()),
        ("notes.txt", b"note".as_slice()),
    ] {
        let fd = sync_api::mudu_fs_open(session_id, oid, entry, O_WRONLY).unwrap();
        sync_api::mudu_fs_write(session_id, fd, content).unwrap();
        sync_api::mudu_fs_close(session_id, fd).unwrap();
    }

    // The object root is virtual: entries are the fs root children matching
    // the `{oidhex}.1.` prefix, collapsed to their first path segment.
    let entries = sync_api::mudu_fs_readdir(session_id, oid, "").unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "docs");
    assert!(entries[0].is_dir);
    assert_eq!(entries[0].length, 0);
    assert_eq!(entries[1].name, "notes.txt");
    assert!(!entries[1].is_dir);
    assert_eq!(entries[1].length, 4);

    // A sub-path names the real host directory.
    let entries = sync_api::mudu_fs_readdir(session_id, oid, "docs").unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "a.txt");
    assert!(!entries[0].is_dir);
    assert_eq!(entries[0].length, 1);
    assert_eq!(entries[1].name, "b.txt");
    assert_eq!(entries[1].length, 2);

    // stat reports files, real directories, and the virtual object root.
    let stat = sync_api::mudu_fs_stat(session_id, oid, "docs/a.txt").unwrap();
    assert_eq!(stat.entry, "docs/a.txt");
    assert_eq!(stat.length, 1);
    assert_eq!(stat.state, 1);
    let stat = sync_api::mudu_fs_stat(session_id, oid, "docs").unwrap();
    assert_eq!(stat.length, 0);
    let stat = sync_api::mudu_fs_stat(session_id, oid, "").unwrap();
    assert_eq!(stat.length, 0);

    sync_api::mudu_close(session_id).unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_standalone_fs_error_paths() {
    let _guard = lock_tests();
    use_temp_db("error_paths");

    let session_id = sync_api::mudu_open().unwrap();
    let oid: u128 = 0xF500_0000_0000_0000_0000_0000_0000_0003;

    // Read-open of an unknown object reports ENOENT.
    let err = sync_api::mudu_fs_open(session_id, oid, "", O_RDONLY).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotFound);

    // O_CREAT (and the other deliberately-unsupported flags) report EINVAL.
    let err = sync_api::mudu_fs_open(session_id, oid, "", O_WRONLY | O_CREAT).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidInput);

    // Open/create the object for the fd-level error cases.
    let fd = sync_api::mudu_fs_open(session_id, oid, "", O_RDWR).unwrap();

    // Read and write payloads are capped at 16 MiB.
    let err = sync_api::mudu_fs_read(session_id, fd, 16 * 1024 * 1024 + 1).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidInput);
    let oversized = vec![0u8; 16 * 1024 * 1024 + 1];
    let err = sync_api::mudu_fs_write(session_id, fd, &oversized).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidInput);

    // lseek rejects unknown whence values and negative results.
    let err = sync_api::mudu_fs_lseek(session_id, fd, 0, 42).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidInput);
    let err = sync_api::mudu_fs_lseek(session_id, fd, -1, SEEK_SET).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidInput);

    sync_api::mudu_fs_close(session_id, fd).unwrap();

    // A closed fd reports EBADF on every fd-based operation.
    let err = sync_api::mudu_fs_read(session_id, fd, 1).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
    let err = sync_api::mudu_fs_close(session_id, fd).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
    let err = sync_api::mudu_fs_fstat(session_id, fd).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);

    // Access-mode violations report EBADF; fsync on a read fd reports EINVAL.
    let fd = sync_api::mudu_fs_open(session_id, oid, "", O_RDONLY).unwrap();
    let err = sync_api::mudu_fs_write(session_id, fd, b"x").unwrap_err();
    assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
    let err = sync_api::mudu_fs_fsync(session_id, fd).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidInput);
    sync_api::mudu_fs_close(session_id, fd).unwrap();

    let fd = sync_api::mudu_fs_open(session_id, oid, "", O_WRONLY).unwrap();
    let err = sync_api::mudu_fs_read(session_id, fd, 1).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
    sync_api::mudu_fs_close(session_id, fd).unwrap();

    sync_api::mudu_close(session_id).unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn async_standalone_fs_smoke() {
    let _guard = lock_tests();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        use_temp_db("async_smoke");

        let session_id = async_api::mudu_open().await.unwrap();
        let oid: u128 = 0xF500_0000_0000_0000_0000_0000_0000_0004;

        let fd = async_api::mudu_fs_open(session_id, oid, "", O_WRONLY)
            .await
            .unwrap();
        assert_eq!(
            async_api::mudu_fs_write(session_id, fd, b"async fs")
                .await
                .unwrap(),
            8
        );
        async_api::mudu_fs_fsync(session_id, fd).await.unwrap();
        async_api::mudu_fs_close(session_id, fd).await.unwrap();

        let fd = async_api::mudu_fs_open(session_id, oid, "", O_RDONLY)
            .await
            .unwrap();
        assert_eq!(
            async_api::mudu_fs_read(session_id, fd, 64).await.unwrap(),
            b"async fs"
        );
        let stat = async_api::mudu_fs_fstat(session_id, fd).await.unwrap();
        assert_eq!(stat.length, 8);
        assert_eq!(stat.generation, 1);
        async_api::mudu_fs_close(session_id, fd).await.unwrap();

        async_api::mudu_close(session_id).await.unwrap();
    });
}
