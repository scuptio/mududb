#![allow(clippy::await_holding_lock)]

use super::*;

// Miri cannot execute the io_uring syscalls used by this recovery test.
#[test]
#[cfg_attr(miri, ignore)]
fn iouring_backend_recovery_replays_worker_logs() -> RS<()> {
    log_setup("info");
    let notifier = NotifyWait::new();
    {
        let _n = notifier.clone();
        let _ = mudu_sys::task::sync::spawn_thread(move || {
            debug_serve(_n, 1800);
        })?;
    };
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
                "mududb_iouring_recovery_{}",
                mudu_sys::random::uuid_v4()
            ));
            mudu_sys::fs::sync::create_dir_all(&data_dir).unwrap();
            let registry = load_or_create_worker_registry(&data_dir, worker_count)?;
            let target_worker_id = registry.worker(0).unwrap().worker_id;

            let (stop_notifier, server_thread) = spawn_iouring_server(
                listeners,
                worker_count,
                &data_dir,
                64 * 1024 * 1024,
                Some(registry.clone()),
            );
            if wait_ready_or_skip_io_uring(port, &server_thread, "skip io_uring recovery test")
                .await?
            {
                return Ok(());
            }
            recovery_seed_and_verify(port, target_worker_id).await?;
            stop_notifier.notify_all();
            server_thread.join().unwrap()?;

            let Some(restart_listeners) = reserve_listener_block(worker_count) else {
                return Ok(());
            };
            let restart_port = restart_listeners[0].local_addr().unwrap().port();
            let (stop_notifier, server_thread) = spawn_iouring_server(
                restart_listeners,
                worker_count,
                &data_dir,
                64 * 1024 * 1024,
                Some(registry.clone()),
            );
            if wait_ready_or_skip_io_uring(
                restart_port,
                &server_thread,
                "skip io_uring recovery restart test",
            )
            .await?
            {
                return Ok(());
            }
            recovery_verify_replayed(restart_port, target_worker_id).await?;
            stop_notifier.notify_all();
            server_thread.join().unwrap()?;
            Ok(())
        })
}

/// Session-create config pinning the session to `worker_id`.
fn recovery_session_config(worker_id: u128) -> String {
    serde_json::json!({
        "session_id": "0",
        "worker_id": worker_id.to_string(),
    })
    .to_string()
}

/// Writes two keys through worker 0 and reads them back before the restart.
async fn recovery_seed_and_verify(port: u16, worker_id: u128) -> RS<()> {
    let mut client = AsyncPerfClient::connect(test_worker_port(port, 0)?).await?;
    client.session_id = client
        .create_session(Some(recovery_session_config(worker_id)))
        .await?;
    client.put(b"alpha".to_vec(), b"one".to_vec()).await?;
    client.put(b"beta".to_vec(), b"two".to_vec()).await?;
    assert_eq!(client.get(b"alpha".to_vec()).await?, Some(b"one".to_vec()));
    assert_eq!(client.get(b"beta".to_vec()).await?, Some(b"two".to_vec()));
    client.close().await?;
    Ok(())
}

/// After the restart, worker log replay must surface the same two keys.
async fn recovery_verify_replayed(port: u16, worker_id: u128) -> RS<()> {
    let mut client = AsyncPerfClient::connect(test_worker_port(port, 0)?).await?;
    client.session_id = client
        .create_session(Some(recovery_session_config(worker_id)))
        .await?;
    assert_eq!(client.get(b"alpha".to_vec()).await?, Some(b"one".to_vec()));
    assert_eq!(client.get(b"beta".to_vec()).await?, Some(b"two".to_vec()));
    client.close().await?;
    Ok(())
}

// Miri cannot execute the io_uring syscalls used by this recovery test.
#[test]
#[cfg_attr(miri, ignore)]
fn iouring_backend_recovery_replays_across_multiple_chunks() -> RS<()> {
    mudu_sys::task::async_::build_multi_thread_runtime()
        .unwrap()
        .block_on(async move {
            if maybe_skip_in_act() {
                return Ok(());
            }
            let _guard = lock_network_perf_test();
            let worker_count = 1usize;
            let log_chunk_size = 64u64;
            let Some(listeners) = reserve_listener_block(worker_count) else {
                return Ok(());
            };
            let port = listeners[0].local_addr().unwrap().port();
            let data_dir = temp_dir().join(format!(
                "mududb_iouring_recovery_multichunk_{}",
                mudu_sys::random::uuid_v4()
            ));
            mudu_sys::fs::sync::create_dir_all(&data_dir).unwrap();
            let entries = vec![
                (b"alpha".to_vec(), b"one".to_vec()),
                (b"beta".to_vec(), b"two".to_vec()),
                (b"gamma".to_vec(), b"three".to_vec()),
                (b"delta".to_vec(), b"four".to_vec()),
            ];

            let (stop_notifier, server_thread) =
                spawn_iouring_server(listeners, worker_count, &data_dir, log_chunk_size, None);
            if let Err(err) = wait_until_server_ready_or_exit_async(port, &server_thread).await {
                if should_skip_iouring_perf(&err) {
                    eprintln!("skip io_uring multichunk recovery test: {}", err);
                    return Ok(());
                }
                return Err(err);
            }
            {
                let mut client = AsyncPerfClient::connect(port).await?;
                for (key, value) in &entries {
                    client.put(key.clone(), value.clone()).await?;
                }
                client.close().await?;
            }
            stop_notifier.notify_all();
            server_thread.join().unwrap()?;

            let chunk_count = mudu_sys::fs::sync::read_dir_entries(&data_dir)
                .unwrap()
                .into_iter()
                .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("xl"))
                .count();
            assert!(
                chunk_count >= 2,
                "expected multiple log chunks, got {}",
                chunk_count
            );

            let Some(restart_listeners) = reserve_listener_block(worker_count) else {
                return Ok(());
            };
            let restart_port = restart_listeners[0].local_addr().unwrap().port();
            let (stop_notifier, server_thread) = spawn_iouring_server(
                restart_listeners,
                worker_count,
                &data_dir,
                log_chunk_size,
                None,
            );
            if let Err(err) =
                wait_until_server_ready_or_exit_async(restart_port, &server_thread).await
            {
                if should_skip_iouring_perf(&err) {
                    eprintln!("skip io_uring multichunk recovery restart test: {}", err);
                    return Ok(());
                }
                return Err(err);
            }
            {
                let mut client = AsyncPerfClient::connect(restart_port).await?;
                for (key, value) in &entries {
                    assert_eq!(client.get(key.clone()).await?, Some(value.clone()));
                }
                client.close().await?;
            }
            stop_notifier.notify_all();
            server_thread.join().unwrap()?;
            Ok(())
        })
}
