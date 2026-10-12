use super::*;

#[tokio::test]
async fn async_relation_syscalls_round_trip() {
    let sid = UniOid::from_oid(7);
    relation_context(sid.to_oid(), relation_worker_local());

    let insert_in = encode_relation_insert_request(
        sid.clone(),
        "district",
        &[(1, &b"w"[..]), (0, &b"d"[..])],
        &[(5, &b"n"[..])],
    );
    let insert_out = async_relation_insert_internal_with_worker_local(insert_in, None).await;
    decode_relation_insert_result(&insert_out).unwrap();

    // Duplicate primary key: EntityAlreadyExists in the error envelope.
    let dup_in = encode_relation_insert_request(
        sid.clone(),
        "district",
        &[(1, &b"w"[..]), (0, &b"d"[..])],
        &[],
    );
    let dup_out = async_relation_insert_internal_with_worker_local(dup_in, None).await;
    let err = decode_relation_insert_result(&dup_out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::EntityAlreadyExists);

    let get_in = encode_relation_get_request(
        sid.clone(),
        "district",
        &[(1, &b"w"[..]), (0, &b"d"[..])],
        &[5, 3],
    );
    let get_out = async_relation_get_internal_with_worker_local(get_in, None).await;
    let row = decode_relation_get_result(&get_out).unwrap();
    assert_eq!(row, Some(vec![Some(b"n".to_vec()), None]));

    let update_in = encode_relation_update_request(
        sid.clone(),
        "district",
        &[(1, &b"w"[..]), (0, &b"d"[..])],
        &[(5, &b"m"[..])],
        &[(5, 0, &b"1"[..]), (4, 1, &b"2"[..])],
    );
    let update_out = async_relation_update_internal_with_worker_local(update_in, None).await;
    assert_eq!(decode_relation_update_result(&update_out).unwrap(), 1);

    let get_in2 = encode_relation_get_request(
        sid.clone(),
        "district",
        &[(1, &b"w"[..]), (0, &b"d"[..])],
        &[5],
    );
    let get_out2 = async_relation_get_internal_with_worker_local(get_in2, None).await;
    assert_eq!(
        decode_relation_get_result(&get_out2).unwrap(),
        Some(vec![Some(b"m".to_vec())])
    );

    // Unknown delta op code: Decode error in the error envelope.
    let bad_op_in = encode_relation_update_request(
        sid.clone(),
        "district",
        &[(1, &b"w"[..]), (0, &b"d"[..])],
        &[],
        &[(5, 9, &b"1"[..])],
    );
    let bad_op_out = async_relation_update_internal_with_worker_local(bad_op_in, None).await;
    let err = decode_relation_update_result(&bad_op_out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    // Missing key: get returns None, update affects zero rows.
    let miss_get_in = encode_relation_get_request(sid.clone(), "district", &[(0, &b"x"[..])], &[5]);
    let miss_get_out = async_relation_get_internal_with_worker_local(miss_get_in, None).await;
    assert_eq!(decode_relation_get_result(&miss_get_out).unwrap(), None);

    Context::remove(sid.to_oid());
}

#[test]
fn sync_relation_syscalls_round_trip() {
    let sid = UniOid::from_oid(8);
    relation_context(sid.to_oid(), relation_worker_local());

    let insert_in =
        encode_relation_insert_request(sid.clone(), "orders", &[(0, &b"o"[..])], &[(3, &b"c"[..])]);
    let insert_out = relation_insert_internal_with_worker_local(&insert_in, None);
    decode_relation_insert_result(&insert_out).unwrap();

    let get_in = encode_relation_get_request(sid.clone(), "orders", &[(0, &b"o"[..])], &[3]);
    let get_out = relation_get_internal_with_worker_local(&get_in, None);
    assert_eq!(
        decode_relation_get_result(&get_out).unwrap(),
        Some(vec![Some(b"c".to_vec())])
    );

    let update_in = encode_relation_update_request(
        sid.clone(),
        "orders",
        &[(0, &b"missing"[..])],
        &[(3, &b"x"[..])],
        &[],
    );
    let update_out = relation_update_internal_with_worker_local(&update_in, None);
    assert_eq!(decode_relation_update_result(&update_out).unwrap(), 0);

    Context::remove(sid.to_oid());
}

#[test]
fn relation_syscalls_require_task_context() {
    // Without a registered task context the frame's session id resolves
    // to nothing and the syscall reports EntityNotFound.
    let missing = UniOid::from_oid(987_654_321);
    let get_in = encode_relation_get_request(missing.clone(), "t", &[(0, &b"k"[..])], &[0]);
    let out = relation_get_internal_with_worker_local(&get_in, None);
    let err = decode_relation_get_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);

    let update_in =
        encode_relation_update_request(missing.clone(), "t", &[(0, &b"k"[..])], &[], &[]);
    let out = relation_update_internal_with_worker_local(&update_in, None);
    let err = decode_relation_update_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);

    let insert_in = encode_relation_insert_request(missing, "t", &[(0, &b"k"[..])], &[]);
    let out = relation_insert_internal_with_worker_local(&insert_in, None);
    let err = decode_relation_insert_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);
}

#[tokio::test]
async fn async_relation_syscalls_require_task_context_and_relation_support() {
    let missing = UniOid::from_oid(987_654_322);
    let get_in = encode_relation_get_request(missing, "t", &[(0, &b"k"[..])], &[0]);
    let out = async_relation_get_internal_with_worker_local(get_in, None).await;
    let err = decode_relation_get_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);

    // The trait default (FakeWorkerLocal does not override relation_*)
    // reports NotImplemented once the task context resolves.
    let sid = UniOid::from_oid(987_654_323);
    relation_context(sid.to_oid(), worker_local());
    let get_in = encode_relation_get_request(sid.clone(), "t", &[(0, &b"k"[..])], &[0]);
    let out = async_relation_get_internal_with_worker_local(get_in, None).await;
    let err = decode_relation_get_result(&out).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotImplemented);

    Context::remove(sid.to_oid());
}
