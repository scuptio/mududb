use super::*;

#[test]
fn open_with_worker_local_mints_session() {
    let wl = worker_local();
    let open_in = encode_open_request(UniOid::from_oid(0));
    let open_out = open_internal_with_worker_local(&open_in, Some(wl.clone()));
    let session_id = decode_open_result(&open_out).unwrap();
    assert_ne!(session_id, 0);
}

#[test]
fn kv_operations_with_worker_local_round_trip() {
    let wl = worker_local();
    let sid = UniOid::from_oid(8_200_010);
    relation_context(sid.to_oid(), wl);

    let put_in = encode_put_request(sid.clone(), b"alpha", b"1");
    let put_out = put_internal_with_worker_local(&put_in, None);
    decode_put_result(&put_out).unwrap();

    let get_in = encode_get_request(sid.clone(), b"alpha");
    let get_out = get_internal_with_worker_local(&get_in, None);
    let value = decode_get_result(&get_out).unwrap();
    assert_eq!(value, Some(b"1".to_vec()));

    let range_in = encode_range_request(sid.clone(), b"a", b"z");
    let range_out = range_internal_with_worker_local(&range_in, None);
    let items = decode_range_result(&range_out).unwrap();
    assert_eq!(items, vec![(b"alpha".to_vec(), b"1".to_vec())]);

    let delete_in = encode_delete_request(sid.clone(), b"alpha");
    let delete_out = delete_internal_with_worker_local(&delete_in, None);
    decode_delete_result(&delete_out).unwrap();

    let get_out2 = get_internal_with_worker_local(&get_in, None);
    assert_eq!(decode_get_result(&get_out2).unwrap(), None);

    Context::remove(sid.to_oid());
}

#[test]
fn kv_operations_without_worker_local_return_error_frames() {
    let get_in = encode_get_request(UniOid::from_oid(1), b"alpha");
    let out = get_internal(&get_in);
    assert!(decode_get_result(&out).is_err());

    let put_in = encode_put_request(UniOid::from_oid(1), b"alpha", b"1");
    let out = put_internal(&put_in);
    assert!(decode_put_result(&out).is_err());

    let delete_in = encode_delete_request(UniOid::from_oid(1), b"alpha");
    let out = delete_internal(&delete_in);
    assert!(decode_delete_result(&out).is_err());

    let range_in = encode_range_request(UniOid::from_oid(1), b"a", b"z");
    let out = range_internal(&range_in);
    assert!(decode_range_result(&out).is_err());
}

fn fs_open_frame_for(session: UniOid) -> Vec<u8> {
    encode_fs_open_request(&UniFsOpenArgv {
        session,
        oid: UniOid::from_oid(2),
        path: "data.bin".to_string(),
        flags: 0,
    })
}

#[test]
fn fs_syscalls_without_task_context_return_error_frames() {
    // fs-open resolves the task context's connection: with no context
    // registered the frame reports EntityNotFound.
    let out =
        fs_open_internal_with_worker_local(&fs_open_frame_for(UniOid::from_oid(8_200_020)), None);
    let err = decode_fs_open_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);

    // The fd/path-based fs syscalls still resolve through the passed
    // worker-local interface, which is absent here.
    let read_in = encode_fs_read_request(3, 16);
    let out = fs_read_internal_with_worker_local(&read_in, None);
    let err = decode_fs_read_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotImplemented);

    let readdir_in = encode_fs_readdir_request(UniOid::from_oid(2), "docs");
    let out = fs_readdir_internal_with_worker_local(&readdir_in, None);
    let err = decode_fs_readdir_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotImplemented);
}

#[test]
fn fs_syscalls_with_default_fs_service_return_not_implemented() {
    // FakeWorkerLocal does not override fs_service(); the trait default
    // reports the fs syscalls as unavailable. fs-open reaches that
    // default through the task context's connection.
    let wl = worker_local();
    let sid = UniOid::from_oid(8_200_021);
    relation_context(sid.to_oid(), wl.clone());
    let out = fs_open_internal_with_worker_local(&fs_open_frame_for(sid.clone()), None);
    let err = decode_fs_open_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotImplemented);
    assert!(err.message().contains("not available on this worker"));
    Context::remove(sid.to_oid());

    let read_in = encode_fs_read_request(3, 16);
    let out = fs_read_internal_with_worker_local(&read_in, Some(wl.clone()));
    let err = decode_fs_read_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotImplemented);

    let readdir_in = encode_fs_readdir_request(UniOid::from_oid(2), "docs");
    let out = fs_readdir_internal_with_worker_local(&readdir_in, Some(wl));
    let err = decode_fs_readdir_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotImplemented);
}

#[test]
fn fs_syscalls_malformed_frames_return_error_frames() {
    // Fewer bytes than the MSSP header: corrupted data.
    let out = fs_open_internal_with_worker_local(&[0x01, 0x02], None);
    let err = decode_fs_open_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::CorruptedData);

    // A valid header with a malformed MessagePack body: decode error.
    let bad_body = encode_frame(MessageKind::FsOpen, &[0xff]);
    let out = fs_open_internal_with_worker_local(&bad_body, None);
    let err = decode_fs_open_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    // A well-formed frame of a different message kind: decode error.
    let wrong_kind = encode_fs_read_request(3, 16);
    let out = fs_open_internal_with_worker_local(&wrong_kind, None);
    let err = decode_fs_open_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    // The same header rules apply to the other fs handlers.
    let out = fs_read_internal_with_worker_local(&[0xff], None);
    let err = decode_fs_read_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::CorruptedData);

    let out = fs_readdir_internal_with_worker_local(&[0xde, 0xad, 0xbe], None);
    let err = decode_fs_readdir_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::CorruptedData);
}

#[tokio::test]
async fn async_kv_operations_with_worker_local_round_trip() {
    let wl = worker_local();
    let sid = UniOid::from_oid(8_200_011);
    relation_context(sid.to_oid(), wl.clone());

    let put_in = encode_put_request(sid.clone(), b"beta", b"2");
    let put_out = async_put_internal_with_worker_local(put_in, None).await;
    decode_put_result(&put_out).unwrap();

    let get_in = encode_get_request(sid.clone(), b"beta");
    let get_out = async_get_internal_with_worker_local(get_in, None).await;
    let value = decode_get_result(&get_out).unwrap();
    assert_eq!(value, Some(b"2".to_vec()));

    let delete_in = encode_delete_request(sid.clone(), b"beta");
    let delete_out = async_delete_internal_with_worker_local(delete_in, None).await;
    decode_delete_result(&delete_out).unwrap();

    let range_in = encode_range_request(sid.clone(), b"a", b"z");
    let range_out = async_range_internal_with_worker_local(range_in, None).await;
    let items = decode_range_result(&range_out).unwrap();
    assert!(items.is_empty());

    // open still mints worker sessions directly through the worker-local.
    let open_in = encode_open_request(UniOid::from_oid(0));
    let open_out = async_open_internal_with_worker_local(open_in, Some(wl.clone())).await;
    let session_id = decode_open_result(&open_out).unwrap();
    assert_ne!(session_id, 0);

    // close resolves the task context and retires the connection's
    // session (opened lazily by the KV operations above).
    let close_in = encode_close_request(sid.clone());
    let close_out = async_close_internal_with_worker_local(close_in, None).await;
    decode_close_result(&close_out).unwrap();

    Context::remove(sid.to_oid());
}

#[tokio::test]
async fn async_kv_operations_without_worker_local_return_error_frames() {
    let get_in = encode_get_request(UniOid::from_oid(1), b"alpha");
    let out = async_get_internal(get_in).await;
    assert!(decode_get_result(&out).is_err());

    let put_in = encode_put_request(UniOid::from_oid(1), b"alpha", b"1");
    let out = async_put_internal(put_in).await;
    assert!(decode_put_result(&out).is_err());

    let delete_in = encode_delete_request(UniOid::from_oid(1), b"alpha");
    let out = async_delete_internal(delete_in).await;
    assert!(decode_delete_result(&out).is_err());

    let range_in = encode_range_request(UniOid::from_oid(1), b"a", b"z");
    let out = async_range_internal(range_in).await;
    assert!(decode_range_result(&out).is_err());
}

#[tokio::test]
async fn async_fs_open_error_paths_return_error_frames() {
    // No task context registered: EntityNotFound in the MSSP error envelope.
    let out = async_fs_open_internal_with_worker_local(
        fs_open_frame_for(UniOid::from_oid(8_200_022)),
        None,
    )
    .await;
    let err = decode_fs_open_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);

    // Task context bound to a worker without an fs service (trait
    // default): NotImplemented.
    let sid = UniOid::from_oid(8_200_023);
    relation_context(sid.to_oid(), worker_local());
    let out = async_fs_open_internal_with_worker_local(fs_open_frame_for(sid.clone()), None).await;
    let err = decode_fs_open_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotImplemented);
    Context::remove(sid.to_oid());

    // Truncated frame: CorruptedData in the MSSP error envelope.
    let out = async_fs_open_internal_with_worker_local(vec![0x01, 0x02], None).await;
    let err = decode_fs_open_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::CorruptedData);
}
