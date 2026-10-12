#![allow(clippy::await_holding_lock)]

use super::*;

// Miri cannot execute the io_uring syscalls used by this session routing test.
#[test]
#[cfg_attr(miri, ignore)]
fn iouring_backend_open_session_routes_connection_to_requested_partition() -> RS<()> {
    mudu_sys::task::async_::build_multi_thread_runtime()
        .unwrap()
        .block_on(async move {
            if maybe_skip_in_act() {
                return Ok(());
            }
            let _guard = lock_network_perf_test();
            let worker_count = 2usize;
            let Some(listeners) = reserve_listener_block(worker_count) else {
                return Ok(());
            };
            let port = listeners[0].local_addr().unwrap().port();
            let data_dir = temp_dir().join(format!(
                "mududb_iouring_route_{}",
                mudu_sys::random::uuid_v4()
            ));
            mudu_sys::fs::sync::create_dir_all(&data_dir).unwrap();

            let target_partition = 1usize;
            let registry = load_or_create_worker_registry(&data_dir, worker_count)?;
            let target_worker = registry.worker(target_partition).unwrap();

            let (stop_notifier, server_thread) = spawn_iouring_server(
                listeners,
                worker_count,
                &data_dir,
                64 * 1024 * 1024,
                Some(registry.clone()),
            );
            if wait_ready_or_skip_io_uring(port, &server_thread, "skip io_uring route test").await?
            {
                return Ok(());
            }
            if wait_ready_or_skip_io_uring(
                test_worker_port(port, target_partition)?,
                &server_thread,
                "skip io_uring route test",
            )
            .await?
            {
                return Ok(());
            }

            {
                let mut client =
                    AsyncPerfClient::connect(test_worker_port(port, target_partition)?).await?;
                let config_json = serde_json::json!({
                    "session_id": "0",
                    "worker_id": target_worker.worker_id.to_string(),
                })
                .to_string();
                client.session_id = create_session_with_retry(&mut client, config_json).await?;
                client
                    .put(b"route-key".to_vec(), b"route-val".to_vec())
                    .await?;
                assert_eq!(
                    client.get(b"route-key".to_vec()).await?,
                    Some(b"route-val".to_vec())
                );
            }

            stop_notifier.notify_all();
            server_thread.join().unwrap()?;

            let routed_chunk_count = count_worker_log_chunks(&data_dir, target_worker.worker_id);
            assert!(
                routed_chunk_count > 0,
                "expected log chunks for target partition {}",
                target_partition
            );
            Ok(())
        })
}

/// Creates the pinned session, retrying briefly while the worker settles.
async fn create_session_with_retry(client: &mut AsyncPerfClient, config_json: String) -> RS<u128> {
    let mut last_err: Option<MuduError> = None;
    let mut session_id = 0u128;
    for _ in 0..3 {
        match client.create_session(Some(config_json.clone())).await {
            Ok(sid) => {
                session_id = sid;
                last_err = None;
                break;
            }
            Err(e) => {
                last_err = Some(e);
                mudu_sys::task::async_::sleep(Duration::from_millis(50)).await?;
            }
        }
    }
    if let Some(e) = last_err {
        return Err(e);
    }
    Ok(session_id)
}

/// Counts the log chunks that carry the short-uuid prefix of `worker_id`.
fn count_worker_log_chunks(data_dir: &std::path::Path, worker_id: u128) -> usize {
    let expected_prefix = ShortUuid::from_uuid(&Uuid::from_u128(worker_id)).to_string();
    mudu_sys::fs::sync::read_dir_entries(data_dir)
        .unwrap()
        .into_iter()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|name| name.starts_with(&expected_prefix) && name.ends_with(".xl"))
                .unwrap_or(false)
        })
        .count()
}

// Miri cannot execute the io_uring syscalls used by this session rebind test.
#[test]
#[cfg_attr(miri, ignore)]
fn iouring_backend_open_session_rebind_keeps_same_session_id() -> RS<()> {
    log_setup("info");
    let notifier = NotifyWait::new();
    {
        let _n = notifier.clone();
        let _ = mudu_sys::task::sync::spawn_thread(move || {
            debug_serve(_n, 1800);
        })?;
    };
    mudu_sys::task::async_::block_on_async_current(async move {
        let trace = task_trace!();
        trace.watch("rebind_test.stage", "start");
        if maybe_skip_in_act() {
            return Ok(());
        }
        trace.watch("rebind_test.stage", "lock_network_perf_test");
        let _guard = lock_network_perf_test();
        trace.watch("rebind_test.stage", "reserve_listener_block");
        let worker_count = 2usize;
        let Some(listeners) = reserve_listener_block(worker_count) else {
            return Ok(());
        };
        let port = listeners[0].local_addr().unwrap().port();
        trace.watch("rebind_test.base_port", &port.to_string());
        let data_dir = temp_dir().join(format!(
            "mududb_iouring_rebind_{}",
            mudu_sys::random::uuid_v4()
        ));
        mudu_sys::fs::sync::create_dir_all(&data_dir).unwrap();

        let target_partition = 1usize;
        let registry = load_or_create_worker_registry(&data_dir, worker_count)?;
        let target_worker = registry.worker(target_partition).unwrap();

        trace.watch("rebind_test.stage", "spawn_iouring_server");
        let (stop_notifier, server_thread) = spawn_iouring_server(
            listeners,
            worker_count,
            &data_dir,
            64 * 1024 * 1024,
            Some(registry.clone()),
        );
        trace.watch("rebind_test.stage", "wait_server_ready");
        if wait_ready_or_skip_io_uring(port, &server_thread, "skip io_uring rebind test").await? {
            return Ok(());
        }
        trace.watch("rebind_test.stage", "wait_target_worker_ready");
        if wait_ready_or_skip_io_uring(
            test_worker_port(port, target_partition)?,
            &server_thread,
            "skip io_uring rebind test",
        )
        .await?
        {
            return Ok(());
        }

        rebind_session_put_get(port, target_partition, target_worker.worker_id).await?;

        trace.watch("rebind_test.stage", "stop_server");
        stop_notifier.notify_all();
        trace.watch("rebind_test.stage", "join_server");
        server_thread.join().unwrap()?;
        trace.watch("rebind_test.stage", "done");
        Ok(())
    })
}

/// Rebinds the connection's session to itself on the target partition, then
/// writes and reads back one key through the rebound session.
async fn rebind_session_put_get(port: u16, target_partition: usize, worker_id: u128) -> RS<()> {
    let trace = task_trace!();
    trace.watch("rebind_test.stage", "client_connect");
    let mut client = AsyncPerfClient::connect(test_worker_port(port, target_partition)?).await?;
    let original_session_id = client.session_id;
    trace.watch(
        "rebind_test.original_session_id",
        &original_session_id.to_string(),
    );
    trace.watch("rebind_test.stage", "client_create_session_rebind");
    let rebound_session_id = client
        .create_session(Some(
            serde_json::json!({
                "session_id": original_session_id.to_string(),
                "worker_id": worker_id.to_string(),
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(rebound_session_id, original_session_id);
    trace.watch("rebind_test.stage", "client_put");
    client
        .put(b"rebind-key".to_vec(), b"rebind-val".to_vec())
        .await?;
    trace.watch("rebind_test.stage", "client_get");
    assert_eq!(
        client.get(b"rebind-key".to_vec()).await?,
        Some(b"rebind-val".to_vec())
    );
    Ok(())
}
