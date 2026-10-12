use super::*;

#[test]
fn sync_kv_syscalls_resolve_session_through_task_context() {
    let fake = session_aware_worker_local();
    let wl: WorkerLocalRef = fake;
    // The frame's session field is a task id that is deliberately not a
    // worker session id: only resolving the task context's connection
    // (and its lazily opened worker session) can complete the operation.
    let task = UniOid::from_oid(8_200_001);
    relation_context(task.to_oid(), wl);

    let put_in = encode_put_request(task.clone(), b"alpha", b"1");
    decode_put_result(&put_internal_with_worker_local(&put_in, None)).unwrap();

    let get_in = encode_get_request(task.clone(), b"alpha");
    let get_out = get_internal_with_worker_local(&get_in, None);
    assert_eq!(decode_get_result(&get_out).unwrap(), Some(b"1".to_vec()));

    let range_in = encode_range_request(task.clone(), b"a", b"z");
    let range_out = range_internal_with_worker_local(&range_in, None);
    assert_eq!(
        decode_range_result(&range_out).unwrap(),
        vec![(b"alpha".to_vec(), b"1".to_vec())]
    );

    let delete_in = encode_delete_request(task.clone(), b"alpha");
    decode_delete_result(&delete_internal_with_worker_local(&delete_in, None)).unwrap();
    let get_out2 = get_internal_with_worker_local(&get_in, None);
    assert_eq!(decode_get_result(&get_out2).unwrap(), None);

    Context::remove(task.to_oid());
}

#[tokio::test]
async fn async_kv_syscalls_resolve_session_through_task_context() {
    let fake = session_aware_worker_local();
    let wl: WorkerLocalRef = fake;
    let task = UniOid::from_oid(8_200_002);
    relation_context(task.to_oid(), wl);

    let put_in = encode_put_request(task.clone(), b"beta", b"2");
    decode_put_result(&async_put_internal_with_worker_local(put_in, None).await).unwrap();

    let get_in = encode_get_request(task.clone(), b"beta");
    let get_out = async_get_internal_with_worker_local(get_in, None).await;
    assert_eq!(decode_get_result(&get_out).unwrap(), Some(b"2".to_vec()));

    let range_in = encode_range_request(task.clone(), b"a", b"z");
    let range_out = async_range_internal_with_worker_local(range_in, None).await;
    assert_eq!(
        decode_range_result(&range_out).unwrap(),
        vec![(b"beta".to_vec(), b"2".to_vec())]
    );

    let delete_in = encode_delete_request(task.clone(), b"beta");
    decode_delete_result(&async_delete_internal_with_worker_local(delete_in, None).await).unwrap();
    let get_out2 =
        async_get_internal_with_worker_local(encode_get_request(task.clone(), b"beta"), None).await;
    assert_eq!(decode_get_result(&get_out2).unwrap(), None);

    Context::remove(task.to_oid());
}

#[test]
fn sync_close_closes_session_through_task_context() {
    let fake = session_aware_worker_local();
    let wl: WorkerLocalRef = fake.clone();
    let task = UniOid::from_oid(8_200_003);
    relation_context(task.to_oid(), wl);

    // Closing a connection that never opened a session is a no-op.
    let close_in = encode_close_request(task.clone());
    decode_close_result(&close_internal_with_worker_local(&close_in, None)).unwrap();

    // A KV operation opens the connection's worker session; close then
    // retires exactly that session.
    let put_in = encode_put_request(task.clone(), b"k", b"v");
    decode_put_result(&put_internal_with_worker_local(&put_in, None)).unwrap();
    assert_eq!(fake.live_session_count(), 1);
    decode_close_result(&close_internal_with_worker_local(&close_in, None)).unwrap();
    assert_eq!(fake.live_session_count(), 0);

    Context::remove(task.to_oid());
}

#[tokio::test]
async fn async_close_closes_session_through_task_context() {
    let fake = session_aware_worker_local();
    let wl: WorkerLocalRef = fake.clone();
    let task = UniOid::from_oid(8_200_004);
    relation_context(task.to_oid(), wl);

    let close_in = encode_close_request(task.clone());
    decode_close_result(&async_close_internal_with_worker_local(close_in.clone(), None).await)
        .unwrap();

    let put_in = encode_put_request(task.clone(), b"k", b"v");
    decode_put_result(&async_put_internal_with_worker_local(put_in, None).await).unwrap();
    assert_eq!(fake.live_session_count(), 1);
    decode_close_result(&async_close_internal_with_worker_local(close_in, None).await).unwrap();
    assert_eq!(fake.live_session_count(), 0);

    Context::remove(task.to_oid());
}

#[test]
fn sync_close_closes_open_minted_session_directly() {
    // The `open` syscall mints worker session ids that are never in the
    // task Context registry; closing such an id goes straight to the
    // worker local.
    let fake = session_aware_worker_local();
    let wl: WorkerLocalRef = fake.clone();
    let session = {
        let fake = fake.clone();
        mudu_utils::task_async::run_async(async move { fake.open_async().await })
            .unwrap()
            .unwrap()
    };
    assert_eq!(fake.live_session_count(), 1);
    let close_in = encode_close_request(UniOid::from_oid(session));
    decode_close_result(&close_internal_with_worker_local(&close_in, Some(wl))).unwrap();
    assert_eq!(fake.live_session_count(), 0);
}

#[tokio::test]
async fn async_close_closes_open_minted_session_directly() {
    let fake = session_aware_worker_local();
    let wl: WorkerLocalRef = fake.clone();
    let session = fake.open_async().await.unwrap();
    assert_eq!(fake.live_session_count(), 1);
    let close_in = encode_close_request(UniOid::from_oid(session));
    decode_close_result(&async_close_internal_with_worker_local(close_in, Some(wl)).await).unwrap();
    assert_eq!(fake.live_session_count(), 0);
}
