//! Exercise every `kernel_function_p2` / `kernel_function_p2_async` host
//! wrapper once: the wrappers are thin forwarders into
//! `kernel_sync`/`kernel_async`, so one call each pins the wiring.
use super::*;
use crate::service::{kernel_function_p2 as p2, kernel_function_p2_async as p2a};
use mudu_binding::system::{command_invoke, query_invoke};

fn wl() -> WorkerLocalRef {
    worker_local()
}

#[test]
fn sync_host_wrappers_forward_sql_and_session() {
    // SQL wrappers need a task context; without one they answer with an
    // error frame (the frame proves the forward happened).
    let query = query_invoke::serialize_query_dyn_param(1u128, &"select 1", &()).unwrap();
    let out = p2::host_query(query.clone());
    assert!(query_invoke::deserialize_query_result(&out).is_err());
    let out = p2::host_command(query.clone());
    assert!(command_invoke::deserialize_command_result(&out).is_err());
    let out = p2::host_batch(query);
    assert!(command_invoke::deserialize_command_result(&out).is_err());

    // Session mint/close via the KV worker-local mock.
    let open_out = p2::host_open(encode_open_request(UniOid::from_oid(0)), Some(wl()));
    let session = decode_open_result(&open_out).unwrap();
    let close_out = p2::host_close(encode_close_request(UniOid::from_oid(session)), Some(wl()));
    decode_close_result(&close_out).unwrap();

    // fetch on an unknown cursor is an error frame, not a panic.
    let fetch_out = p2::host_fetch(vec![0x90]);
    let _ = fetch_out;
}

#[test]
fn sync_host_wrappers_forward_kv_relation_and_fs() {
    // KV wrappers resolve the session through the task context.
    let sid = UniOid::from_oid(41);
    let w = wl();
    relation_context(sid.to_oid(), w);
    let put_out = p2::host_put(encode_put_request(sid.clone(), b"k", b"v"), None);
    decode_put_result(&put_out).unwrap();
    let get_out = p2::host_get(encode_get_request(sid.clone(), b"k"), None);
    assert_eq!(decode_get_result(&get_out).unwrap(), Some(b"v".to_vec()));
    let range_out = p2::host_range(encode_range_request(sid.clone(), b"a", b"z"), None);
    assert_eq!(decode_range_result(&range_out).unwrap().len(), 1);
    let del_out = p2::host_delete(encode_delete_request(sid.clone(), b"k"), None);
    decode_delete_result(&del_out).unwrap();
    Context::remove(sid.to_oid());

    // Relation wrappers against the in-memory relation store.
    let sid = UniOid::from_oid(42);
    let rel = relation_worker_local();
    relation_context(sid.to_oid(), rel.clone());
    let ins = p2::host_relation_insert(
        encode_relation_insert_request(sid.clone(), "t", &[(0, &b"k"[..])], &[(1, &b"v"[..])]),
        Some(rel.clone()),
    );
    decode_relation_insert_result(&ins).unwrap();
    let get = p2::host_relation_get(
        encode_relation_get_request(sid.clone(), "t", &[(0, &b"k"[..])], &[1]),
        Some(rel.clone()),
    );
    assert_eq!(
        decode_relation_get_result(&get).unwrap(),
        Some(vec![Some(b"v".to_vec())])
    );
    let upd = p2::host_relation_update(
        encode_relation_update_request(sid.clone(), "t", &[(0, &b"k"[..])], &[(1, &b"w"[..])], &[]),
        Some(rel.clone()),
    );
    assert_eq!(decode_relation_update_result(&upd).unwrap(), 1);
    Context::remove(sid.to_oid());

    // fs wrappers against the in-memory fs service.
    let sid = UniOid::from_oid(43);
    let fwl = fs_worker_local();
    task_context(sid.to_oid(), fwl.clone());
    let fs_open = p2::host_fs_open(
        encode_fs_open_request(&UniFsOpenArgv {
            session: sid.clone(),
            oid: UniOid::from_oid(2),
            path: "data.bin".to_string(),
            flags: 0,
        }),
        None,
    );
    let fd = decode_fs_open_result(&fs_open).unwrap();
    let out = p2::host_fs_write(encode_fs_write_request(fd, b"AB"), Some(fwl.clone()));
    decode_fs_write_result(&out).unwrap();
    let out = p2::host_fs_lseek(encode_fs_lseek_request(fd, 0, 0), Some(fwl.clone()));
    decode_fs_lseek_result(&out).unwrap();
    let out = p2::host_fs_read(encode_fs_read_request(fd, 8), Some(fwl.clone()));
    assert_eq!(decode_fs_read_result(&out).unwrap(), b"ABllo-fs".to_vec());
    let out = p2::host_fs_pread(encode_fs_pread_request(fd, 0, 2), Some(fwl.clone()));
    assert_eq!(decode_fs_pread_result(&out).unwrap(), b"AB".to_vec());
    let out = p2::host_fs_pwrite(encode_fs_pwrite_request(fd, 0, b"zz"), Some(fwl.clone()));
    decode_fs_pwrite_result(&out).unwrap();
    let out = p2::host_fs_fstat(encode_fs_fstat_request(fd), Some(fwl.clone()));
    assert_eq!(decode_fs_fstat_result(&out).unwrap().length, 8);
    let out = p2::host_fs_fsync(encode_fs_fsync_request(fd), Some(fwl.clone()));
    decode_fs_fsync_result(&out).unwrap();
    let out = p2::host_fs_stat(
        encode_fs_stat_request(UniOid::from_oid(2), "data.bin"),
        Some(fwl.clone()),
    );
    assert_eq!(decode_fs_stat_result(&out).unwrap().length, 8);
    let out = p2::host_fs_readdir(
        encode_fs_readdir_request(UniOid::from_oid(2), "/"),
        Some(fwl.clone()),
    );
    assert_eq!(decode_fs_readdir_result(&out).unwrap().len(), 1);
    let out = p2::host_fs_close(encode_fs_close_request(fd), Some(fwl.clone()));
    decode_fs_close_result(&out).unwrap();
    Context::remove(sid.to_oid());
}

#[tokio::test]
async fn async_host_wrappers_forward_sql_session_and_kv() {
    let query = query_invoke::serialize_query_dyn_param(1u128, &"select 1", &()).unwrap();
    let out = p2a::async_host_query(query.clone()).await;
    assert!(query_invoke::deserialize_query_result(&out).is_err());
    let out = p2a::async_host_command(query.clone()).await;
    assert!(command_invoke::deserialize_command_result(&out).is_err());
    let out = p2a::async_host_batch(query).await;
    assert!(command_invoke::deserialize_command_result(&out).is_err());

    let open_out = p2a::async_host_open(encode_open_request(UniOid::from_oid(0)), Some(wl())).await;
    let session = decode_open_result(&open_out).unwrap();
    let close_out =
        p2a::async_host_close(encode_close_request(UniOid::from_oid(session)), Some(wl())).await;
    decode_close_result(&close_out).unwrap();

    let fetch_out = p2a::async_host_fetch(vec![0x90]).await;
    let _ = fetch_out;

    let sid = UniOid::from_oid(51);
    let w = wl();
    relation_context(sid.to_oid(), w);
    let out = p2a::async_host_put(encode_put_request(sid.clone(), b"k", b"v"), None).await;
    decode_put_result(&out).unwrap();
    let out = p2a::async_host_get(encode_get_request(sid.clone(), b"k"), None).await;
    assert_eq!(decode_get_result(&out).unwrap(), Some(b"v".to_vec()));
    let out = p2a::async_host_range(encode_range_request(sid.clone(), b"a", b"z"), None).await;
    assert_eq!(decode_range_result(&out).unwrap().len(), 1);
    let out = p2a::async_host_delete(encode_delete_request(sid.clone(), b"k"), None).await;
    decode_delete_result(&out).unwrap();
    Context::remove(sid.to_oid());
}

#[tokio::test]
async fn async_host_wrappers_forward_relation_and_fs() {
    let sid = UniOid::from_oid(52);
    let rel = relation_worker_local();
    relation_context(sid.to_oid(), rel.clone());
    let out = p2a::async_host_relation_insert(
        encode_relation_insert_request(sid.clone(), "t", &[(0, &b"k"[..])], &[(1, &b"v"[..])]),
        Some(rel.clone()),
    )
    .await;
    decode_relation_insert_result(&out).unwrap();
    let out = p2a::async_host_relation_get(
        encode_relation_get_request(sid.clone(), "t", &[(0, &b"k"[..])], &[1]),
        Some(rel.clone()),
    )
    .await;
    assert_eq!(
        decode_relation_get_result(&out).unwrap(),
        Some(vec![Some(b"v".to_vec())])
    );
    let out = p2a::async_host_relation_update(
        encode_relation_update_request(sid.clone(), "t", &[(0, &b"k"[..])], &[(1, &b"w"[..])], &[]),
        Some(rel.clone()),
    )
    .await;
    assert_eq!(decode_relation_update_result(&out).unwrap(), 1);
    Context::remove(sid.to_oid());

    let sid = UniOid::from_oid(53);
    let fwl = fs_worker_local();
    task_context(sid.to_oid(), fwl.clone());
    let out = p2a::async_host_fs_open(
        encode_fs_open_request(&UniFsOpenArgv {
            session: sid.clone(),
            oid: UniOid::from_oid(2),
            path: "data.bin".to_string(),
            flags: 0,
        }),
        None,
    )
    .await;
    let fd = decode_fs_open_result(&out).unwrap();
    let out = p2a::async_host_fs_write(encode_fs_write_request(fd, b"AB"), Some(fwl.clone())).await;
    decode_fs_write_result(&out).unwrap();
    let out = p2a::async_host_fs_read(encode_fs_read_request(fd, 4), Some(fwl.clone())).await;
    assert_eq!(decode_fs_read_result(&out).unwrap(), b"llo-".to_vec());
    let out = p2a::async_host_fs_lseek(encode_fs_lseek_request(fd, 0, 0), Some(fwl.clone())).await;
    decode_fs_lseek_result(&out).unwrap();
    let out = p2a::async_host_fs_pread(encode_fs_pread_request(fd, 0, 2), Some(fwl.clone())).await;
    assert_eq!(decode_fs_pread_result(&out).unwrap(), b"AB".to_vec());
    let out =
        p2a::async_host_fs_pwrite(encode_fs_pwrite_request(fd, 0, b"zz"), Some(fwl.clone())).await;
    decode_fs_pwrite_result(&out).unwrap();
    let out = p2a::async_host_fs_fstat(encode_fs_fstat_request(fd), Some(fwl.clone())).await;
    decode_fs_fstat_result(&out).unwrap();
    let out = p2a::async_host_fs_fsync(encode_fs_fsync_request(fd), Some(fwl.clone())).await;
    decode_fs_fsync_result(&out).unwrap();
    let out = p2a::async_host_fs_stat(
        encode_fs_stat_request(UniOid::from_oid(2), "data.bin"),
        Some(fwl.clone()),
    )
    .await;
    decode_fs_stat_result(&out).unwrap();
    let out = p2a::async_host_fs_readdir(
        encode_fs_readdir_request(UniOid::from_oid(2), "/"),
        Some(fwl.clone()),
    )
    .await;
    decode_fs_readdir_result(&out).unwrap();
    let out = p2a::async_host_fs_close(encode_fs_close_request(fd), Some(fwl.clone())).await;
    decode_fs_close_result(&out).unwrap();
    Context::remove(sid.to_oid());
}
