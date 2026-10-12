#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn fs_open_frames_roundtrip() {
    let frame = encode_fs_open_request(&sample_fs_open_argv());
    check_request(MessageKind::FsOpen, &frame, decode_fs_open_request, |v| {
        encode_fs_open_request(&v)
    });
    let argv = decode_fs_open_request(&frame).unwrap();
    assert_eq!(argv.path, "/tmp/file");
    assert_eq!(argv.flags, 3);

    let ok_frame = encode_fs_open_result(&Ok(7));
    let err_frame = encode_fs_open_result(&Err(not_found_error()));
    check_result(
        MessageKind::FsOpen,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_fs_open_result,
        |v| encode_fs_open_result(&Ok(v)),
    );
    assert_eq!(decode_fs_open_result(&ok_frame).unwrap(), 7);
}

#[test]
fn fs_close_frames_roundtrip() {
    let frame = encode_fs_close_request(7);
    check_request(
        MessageKind::FsClose,
        &frame,
        decode_fs_close_request,
        encode_fs_close_request,
    );
    check_unit_result(
        MessageKind::FsClose,
        encode_fs_close_result,
        decode_fs_close_result,
        ErrorCode::BadFileDescriptor,
    );
}

#[test]
fn fs_read_frames_roundtrip() {
    let frame = encode_fs_read_request(7, 128);
    check_request(
        MessageKind::FsRead,
        &frame,
        decode_fs_read_request,
        |(fd, len)| encode_fs_read_request(fd, len),
    );
    assert_eq!(decode_fs_read_request(&frame).unwrap(), (7, 128));

    let ok_frame = encode_fs_read_result(&Ok(b"data".to_vec()));
    let err_frame = encode_fs_read_result(&Err(bad_fd_error()));
    check_result(
        MessageKind::FsRead,
        &ok_frame,
        &err_frame,
        ErrorCode::BadFileDescriptor,
        decode_fs_read_result,
        |v| encode_fs_read_result(&Ok(v)),
    );
    assert_eq!(decode_fs_read_result(&ok_frame).unwrap(), b"data");
}

#[test]
fn fs_write_frames_roundtrip() {
    let frame = encode_fs_write_request(7, b"abc");
    check_request(
        MessageKind::FsWrite,
        &frame,
        decode_fs_write_request,
        |(fd, data)| encode_fs_write_request(fd, &data),
    );
    let (fd, data) = decode_fs_write_request(&frame).unwrap();
    assert_eq!(fd, 7);
    assert_eq!(data, b"abc");

    let ok_frame = encode_fs_write_result(&Ok(3));
    let err_frame = encode_fs_write_result(&Err(bad_fd_error()));
    check_result(
        MessageKind::FsWrite,
        &ok_frame,
        &err_frame,
        ErrorCode::BadFileDescriptor,
        decode_fs_write_result,
        |v| encode_fs_write_result(&Ok(v)),
    );
    assert_eq!(decode_fs_write_result(&ok_frame).unwrap(), 3);
}

#[test]
fn fs_pread_frames_roundtrip() {
    let frame = encode_fs_pread_request(7, 4096, 512);
    check_request(
        MessageKind::FsPread,
        &frame,
        decode_fs_pread_request,
        |(fd, offset, len)| encode_fs_pread_request(fd, offset, len),
    );
    assert_eq!(decode_fs_pread_request(&frame).unwrap(), (7, 4096, 512));

    let ok_frame = encode_fs_pread_result(&Ok(b"data".to_vec()));
    let err_frame = encode_fs_pread_result(&Err(bad_fd_error()));
    check_result(
        MessageKind::FsPread,
        &ok_frame,
        &err_frame,
        ErrorCode::BadFileDescriptor,
        decode_fs_pread_result,
        |v| encode_fs_pread_result(&Ok(v)),
    );
}

#[test]
fn fs_pwrite_frames_roundtrip() {
    let frame = encode_fs_pwrite_request(7, 4096, b"abc");
    check_request(
        MessageKind::FsPwrite,
        &frame,
        decode_fs_pwrite_request,
        |(fd, offset, data)| encode_fs_pwrite_request(fd, offset, &data),
    );
    let (fd, offset, data) = decode_fs_pwrite_request(&frame).unwrap();
    assert_eq!((fd, offset), (7, 4096));
    assert_eq!(data, b"abc");
    check_unit_result(
        MessageKind::FsPwrite,
        encode_fs_pwrite_result,
        decode_fs_pwrite_result,
        ErrorCode::BadFileDescriptor,
    );
}

#[test]
fn fs_lseek_frames_roundtrip() {
    let frame = encode_fs_lseek_request(7, -5, 2);
    check_request(
        MessageKind::FsLseek,
        &frame,
        decode_fs_lseek_request,
        |(fd, offset, whence)| encode_fs_lseek_request(fd, offset, whence),
    );
    assert_eq!(decode_fs_lseek_request(&frame).unwrap(), (7, -5, 2));

    let ok_frame = encode_fs_lseek_result(&Ok(1024));
    let err_frame = encode_fs_lseek_result(&Err(bad_fd_error()));
    check_result(
        MessageKind::FsLseek,
        &ok_frame,
        &err_frame,
        ErrorCode::BadFileDescriptor,
        decode_fs_lseek_result,
        |v| encode_fs_lseek_result(&Ok(v)),
    );
    assert_eq!(decode_fs_lseek_result(&ok_frame).unwrap(), 1024);
}

#[test]
fn fs_fstat_frames_roundtrip() {
    let frame = encode_fs_fstat_request(7);
    check_request(
        MessageKind::FsFstat,
        &frame,
        decode_fs_fstat_request,
        encode_fs_fstat_request,
    );

    let ok_frame = encode_fs_fstat_result(&Ok(sample_fs_stat()));
    let err_frame = encode_fs_fstat_result(&Err(bad_fd_error()));
    check_result(
        MessageKind::FsFstat,
        &ok_frame,
        &err_frame,
        ErrorCode::BadFileDescriptor,
        decode_fs_fstat_result,
        |v| encode_fs_fstat_result(&Ok(v)),
    );
    let stat = decode_fs_fstat_result(&ok_frame).unwrap();
    assert_eq!(stat.length, 100);
    assert_eq!(stat.oid.to_oid(), sample_oid_u128());
}

#[test]
fn fs_stat_frames_roundtrip() {
    let frame = encode_fs_stat_request(sample_oid(), "/tmp/file");
    check_request(
        MessageKind::FsStat,
        &frame,
        decode_fs_stat_request,
        |(oid, path)| encode_fs_stat_request(oid, &path),
    );
    let (oid, path) = decode_fs_stat_request(&frame).unwrap();
    assert_eq!(oid.to_oid(), sample_oid_u128());
    assert_eq!(path, "/tmp/file");

    let ok_frame = encode_fs_stat_result(&Ok(sample_fs_stat()));
    let err_frame = encode_fs_stat_result(&Err(not_found_error()));
    check_result(
        MessageKind::FsStat,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_fs_stat_result,
        |v| encode_fs_stat_result(&Ok(v)),
    );
}

#[test]
fn fs_fsync_frames_roundtrip() {
    let frame = encode_fs_fsync_request(7);
    check_request(
        MessageKind::FsFsync,
        &frame,
        decode_fs_fsync_request,
        encode_fs_fsync_request,
    );
    check_unit_result(
        MessageKind::FsFsync,
        encode_fs_fsync_result,
        decode_fs_fsync_result,
        ErrorCode::BadFileDescriptor,
    );
}

#[test]
fn fs_readdir_frames_roundtrip() {
    let frame = encode_fs_readdir_request(sample_oid(), "/");
    check_request(
        MessageKind::FsReaddir,
        &frame,
        decode_fs_readdir_request,
        |(oid, path)| encode_fs_readdir_request(oid, &path),
    );
    let (_, path) = decode_fs_readdir_request(&frame).unwrap();
    assert_eq!(path, "/");

    let ok_frame = encode_fs_readdir_result(&Ok(sample_readdir_entries()));
    let err_frame = encode_fs_readdir_result(&Err(not_found_error()));
    check_result(
        MessageKind::FsReaddir,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_fs_readdir_result,
        |v| encode_fs_readdir_result(&Ok(v)),
    );
    let entries = decode_fs_readdir_result(&ok_frame).unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries[0].is_dir);
    assert_eq!(entries[1].length, 12);
}
