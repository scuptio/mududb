use crate::server_ur::routing::{route_worker, RoutingContext, RoutingMode};
use crate::server_ur::server::{IoUringTcpBackend, IoUringTcpServerConfig};
use crate::server_ur::worker_registry::load_or_create_worker_registry;
use log::info;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::error::err::MError;
use mudu::m_error;
use mudu_contract::protocol::{
    decode_error_response, decode_get_response, decode_put_response,
    decode_session_create_response, encode_get_request, encode_put_request,
    encode_session_create_request, Frame, GetRequest, MessageType, PutRequest,
    SessionCreateRequest, HEADER_LEN,
};
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{notify_wait, NotifyWait};
use mudu_utils::sync::unique_inner::UniqueInner;
use mudu_utils::task::{spawn_local_task, spawn_task};
use mudu_utils::{debug, task_trace};
use pgwire::tokio::tokio_rustls::rustls::internal::msgs::base::Payload;
use short_uuid::ShortUuid;
use std::env::temp_dir;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream as TokioTcpStream;
use tokio::sync::Barrier;
use tokio::task::JoinSet;
use tracing::debug;
use uuid::Uuid;

struct AsyncPerfClient {
    stream: TokioTcpStream,
    next_request_id: u64,
    session_id: u128,
}

impl AsyncPerfClient {
    async fn connect(port: u16) -> RS<Self> {
        let stream = TokioTcpStream::connect(("127.0.0.1", port))
            .await
            .map_err(|e| m_error!(EC::NetErr, "connect io_uring tcp server error", e))?;
        stream
            .set_nodelay(true)
            .map_err(|e| m_error!(EC::NetErr, "set tcp nodelay error", e))?;
        let mut client = Self {
            stream,
            next_request_id: 1,
            session_id: 0,
        };
        client.session_id = client.create_session(None).await?;
        Ok(client)
    }

    async fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        let _ = task_trace!();

        let request_id = self.take_request_id();
        let payload =
            encode_put_request(request_id, &PutRequest::new(self.session_id, key, value))?;
        let frame = self.send_and_receive(&payload).await?;
        self.ensure_success_frame(&frame)?;
        if decode_put_response(&frame)?.ok() {
            Ok(())
        } else {
            Err(m_error!(
                EC::NetErr,
                "remote put operation returned failure"
            ))
        }
    }

    async fn get(&mut self, key: Vec<u8>) -> RS<Option<Vec<u8>>> {
        let _t = task_trace!();
        let request_id = self.take_request_id();
        let payload = encode_get_request(request_id, &GetRequest::new(self.session_id, key))?;
        let frame = self.send_and_receive(&payload).await?;
        self.ensure_success_frame(&frame)?;
        Ok(decode_get_response(&frame)?.into_value())
    }

    async fn create_session(&mut self, config_json: Option<String>) -> RS<u128> {
        let request_id = self.take_request_id();
        let payload =
            encode_session_create_request(request_id, &SessionCreateRequest::new(config_json))?;
        let frame = self.send_and_receive(&payload).await?;
        self.ensure_success_frame(&frame)?;
        Ok(decode_session_create_response(&frame)?.session_id())
    }

    fn take_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        request_id
    }

    async fn send_and_receive(&mut self, payload: &[u8]) -> RS<Frame> {
        let _trace = task_trace!();
        self._send(payload).await?;
        self._receive().await
    }

    async fn _send(&mut self, payload: &[u8]) -> RS<()> {
        let _trace = task_trace!();
        self.stream
            .write_all(payload)
            .await
            .map_err(|e| m_error!(EC::NetErr, "write request frame error", e))?;
        self.stream
            .flush()
            .await
            .map_err(|e| m_error!(EC::NetErr, "flush request frame error", e))?;
        Ok(())
    }
    async fn _receive(&mut self) -> RS<Frame> {
        let _ = task_trace!();
        let mut header = [0u8; HEADER_LEN];
        self.stream
            .read_exact(&mut header)
            .await
            .map_err(|e| m_error!(EC::NetErr, "read response header error", e))?;
        let payload_len =
            u32::from_be_bytes([header[16], header[17], header[18], header[19]]) as usize;
        let mut frame_bytes = Vec::with_capacity(HEADER_LEN + payload_len);
        frame_bytes.extend_from_slice(&header);
        if payload_len > 0 {
            let mut body = vec![0u8; payload_len];
            self.stream
                .read_exact(&mut body)
                .await
                .map_err(|e| m_error!(EC::NetErr, "read response payload error", e))?;
            frame_bytes.extend_from_slice(&body);
        }
        Frame::decode(&frame_bytes)
    }

    fn ensure_success_frame(&self, frame: &Frame) -> RS<()> {
        let _trace = task_trace!();
        if frame.header().message_type() == MessageType::Error {
            let error = decode_error_response(frame)?;
            return Err(m_error!(EC::NetErr, error.message()));
        }
        Ok(())
    }
}

fn reserve_port() -> Option<u16> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("skip io_uring perf test: {err}");
            return None;
        }
    };
    Some(listener.local_addr().ok()?.port())
}

async fn wait_until_server_ready_async(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if AsyncPerfClient::connect(port).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("io_uring backend did not become ready on port {}", port);
}

fn spawn_iouring_server(
    port: u16,
    worker_count: usize,
    data_dir: &std::path::Path,
    log_chunk_size: u64,
) -> (mudu_utils::notifier::Notifier, thread::JoinHandle<RS<()>>) {
    let (stop_notifier, server_stop) = notify_wait();
    let server_cfg = IoUringTcpServerConfig::new(
        worker_count,
        "127.0.0.1".to_string(),
        port,
        data_dir.to_string_lossy().into_owned(),
        RoutingMode::ConnectionId,
        None,
    )
    .unwrap()
    .with_log_chunk_size(log_chunk_size);
    let server_thread =
        thread::spawn(move || IoUringTcpBackend::sync_serve_with_stop(server_cfg, server_stop));
    (stop_notifier, server_thread)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn iouring_backend_perf_put_get() -> RS<()> {
    log_setup("info");
    let notifier = NotifyWait::new();
    {
        let _n = notifier.clone();
        let _ = thread::spawn(move || {
            debug::debug_serve(_n, 1800);
        });
    };
    let Some(port) = reserve_port() else {
        return Ok(());
    };
    let worker_count = 6usize;
    let clients = 6usize;
    let bench_duration = Duration::from_secs(10);
    let data_dir = temp_dir().join(format!("mududb_iouring_perf_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();

    let (stop_notifier, server_stop) = notifier.notify_wait();
    let server_cfg = IoUringTcpServerConfig::new(
        worker_count,
        "127.0.0.1".to_string(),
        port,
        data_dir.to_string_lossy().into_owned(),
        RoutingMode::ConnectionId,
        None,
    )
    .unwrap();
    let server_thread =
        thread::spawn(move || IoUringTcpBackend::sync_serve_with_stop(server_cfg, server_stop));

    wait_until_server_ready_async(port).await;

    let start_barrier = Arc::new(Barrier::new(clients + 1));
    let stop_clients = Arc::new(AtomicBool::new(false));
    let put_ops = Arc::new(AtomicU64::new(0));
    let get_ops = Arc::new(AtomicU64::new(0));
    let mut join_set: JoinSet<RS<()>> = tokio::task::JoinSet::new();
    for client_id in 0..clients {
        let start_barrier = start_barrier.clone();
        let stop_clients = stop_clients.clone();
        let put_ops = put_ops.clone();
        let get_ops = get_ops.clone();
        let join_handle = spawn_task(
            notifier.clone(),
            format!("task_cli_{}", client_id).as_str(),
            async move {
                let mut client = AsyncPerfClient::connect(port).await?;
                start_barrier.wait().await;
                let mut op_id = 0usize;
                while !stop_clients.load(Ordering::Relaxed) {
                    let key = format!("client-{client_id:02}-key-{op_id:06}").into_bytes();
                    let value = format!("value-{client_id:02}-{op_id:06}").into_bytes();
                    debug!("client {} put key ", client_id);
                    client.put(key.clone(), value.clone()).await?;
                    debug!("client {} put key done", client_id);
                    put_ops.fetch_add(1, Ordering::Relaxed);
                    debug!("client {} get key", client_id);
                    let returned = client.get(key).await?;
                    debug!("client {} get key done", client_id);
                    assert_eq!(returned, Some(value));
                    get_ops.fetch_add(1, Ordering::Relaxed);
                    op_id += 1;
                }
                Ok::<(), MError>(())
            },
        )?;
        join_set.spawn(async move {
            join_handle.await;
            Ok::<(), MError>(())
        });
    }

    start_barrier.wait().await;
    info!("begin testing");
    let started_at = Instant::now();
    tokio::time::sleep(bench_duration).await;
    let elapsed = started_at.elapsed();
    stop_clients.store(true, Ordering::Relaxed);

    let total_put_ops = put_ops.load(Ordering::Relaxed) as usize;
    let total_get_ops = get_ops.load(Ordering::Relaxed) as usize;
    let total_ops = total_put_ops + total_get_ops;
    let throughput = total_ops as f64 / elapsed.as_secs_f64();
    info!(
        "io_uring kv perf: clients={}, puts={}, gets={}, total_ops={}, elapsed_ms={}, throughput_ops_per_sec={:.2}",
        clients,
        total_put_ops,
        total_get_ops,
        total_ops,
        elapsed.as_millis(),
        throughput
    );

    while let Some(result) = join_set.join_next().await {
        result.unwrap()?;
    }
    stop_notifier.notify_all();
    server_thread.join().unwrap()?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn iouring_backend_recovery_replays_worker_logs() -> RS<()> {
    let Some(port) = reserve_port() else {
        return Ok(());
    };
    let worker_count = 2usize;
    let data_dir = temp_dir().join(format!("mududb_iouring_recovery_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();
    let registry = load_or_create_worker_registry(&data_dir, worker_count)?;
    let target_worker = registry.worker(0).unwrap();

    let (stop_notifier, server_thread) =
        spawn_iouring_server(port, worker_count, &data_dir, 64 * 1024 * 1024);
    wait_until_server_ready_async(port).await;

    {
        let mut client = AsyncPerfClient::connect(port).await?;
        client.session_id = client
            .create_session(Some(
                serde_json::json!({
                    "session_id": "0",
                    "worker_id": target_worker.worker_id.to_string(),
                })
                .to_string(),
            ))
            .await?;
        client.put(b"alpha".to_vec(), b"one".to_vec()).await?;
        client.put(b"beta".to_vec(), b"two".to_vec()).await?;
        assert_eq!(client.get(b"alpha".to_vec()).await?, Some(b"one".to_vec()));
        assert_eq!(client.get(b"beta".to_vec()).await?, Some(b"two".to_vec()));
    }

    stop_notifier.notify_all();
    server_thread.join().unwrap()?;

    let (stop_notifier, server_thread) =
        spawn_iouring_server(port, worker_count, &data_dir, 64 * 1024 * 1024);
    wait_until_server_ready_async(port).await;

    {
        let mut client = AsyncPerfClient::connect(port).await?;
        client.session_id = client
            .create_session(Some(
                serde_json::json!({
                    "session_id": "0",
                    "worker_id": target_worker.worker_id.to_string(),
                })
                .to_string(),
            ))
            .await?;
        assert_eq!(client.get(b"alpha".to_vec()).await?, Some(b"one".to_vec()));
        assert_eq!(client.get(b"beta".to_vec()).await?, Some(b"two".to_vec()));
    }

    stop_notifier.notify_all();
    server_thread.join().unwrap()?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn iouring_backend_recovery_replays_across_multiple_chunks() -> RS<()> {
    let Some(port) = reserve_port() else {
        return Ok(());
    };
    let worker_count = 1usize;
    let log_chunk_size = 64u64;
    let data_dir = temp_dir().join(format!(
        "mududb_iouring_recovery_multichunk_{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&data_dir).unwrap();
    let entries = vec![
        (b"alpha".to_vec(), b"one".to_vec()),
        (b"beta".to_vec(), b"two".to_vec()),
        (b"gamma".to_vec(), b"three".to_vec()),
        (b"delta".to_vec(), b"four".to_vec()),
    ];

    let (stop_notifier, server_thread) =
        spawn_iouring_server(port, worker_count, &data_dir, log_chunk_size);
    wait_until_server_ready_async(port).await;
    {
        let mut client = AsyncPerfClient::connect(port).await?;
        for (key, value) in &entries {
            client.put(key.clone(), value.clone()).await?;
        }
    }
    stop_notifier.notify_all();
    server_thread.join().unwrap()?;

    let chunk_count = std::fs::read_dir(&data_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("xl"))
        .count();
    assert!(
        chunk_count >= 2,
        "expected multiple log chunks, got {}",
        chunk_count
    );

    let (stop_notifier, server_thread) =
        spawn_iouring_server(port, worker_count, &data_dir, log_chunk_size);
    wait_until_server_ready_async(port).await;
    {
        let mut client = AsyncPerfClient::connect(port).await?;
        for (key, value) in &entries {
            assert_eq!(client.get(key.clone()).await?, Some(value.clone()));
        }
    }
    stop_notifier.notify_all();
    server_thread.join().unwrap()?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn iouring_backend_open_session_routes_connection_to_requested_partition() -> RS<()> {
    let Some(port) = reserve_port() else {
        return Ok(());
    };
    let worker_count = 2usize;
    let data_dir = temp_dir().join(format!("mududb_iouring_route_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();

    let initial_partition = route_worker(
        &RoutingContext::new(1, "127.0.0.1:10000".parse().unwrap(), None),
        RoutingMode::ConnectionId,
        worker_count,
    );
    let target_partition = (initial_partition + 1) % worker_count;
    let registry = load_or_create_worker_registry(&data_dir, worker_count)?;
    let target_worker = registry.worker(target_partition).unwrap();

    let (stop_notifier, server_thread) =
        spawn_iouring_server(port, worker_count, &data_dir, 64 * 1024 * 1024);
    wait_until_server_ready_async(port).await;

    {
        let mut client = AsyncPerfClient::connect(port).await?;
        let session_id = client
            .create_session(Some(
                serde_json::json!({
                    "session_id": "0",
                    "worker_id": target_worker.worker_id.to_string(),
                })
                .to_string(),
            ))
            .await?;
        client.session_id = session_id;
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

    let expected_prefix =
        ShortUuid::from_uuid(&Uuid::from_u128(target_worker.worker_id)).to_string();
    let routed_chunk_count = std::fs::read_dir(&data_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|name| name.starts_with(&expected_prefix) && name.ends_with(".xl"))
                .unwrap_or(false)
        })
        .count();
    assert!(
        routed_chunk_count > 0,
        "expected log chunks for target partition {}",
        target_partition
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn iouring_backend_open_session_rebind_keeps_same_session_id() -> RS<()> {
    let Some(port) = reserve_port() else {
        return Ok(());
    };
    let worker_count = 2usize;
    let data_dir = temp_dir().join(format!("mududb_iouring_rebind_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();

    let initial_partition = route_worker(
        &RoutingContext::new(1, "127.0.0.1:10001".parse().unwrap(), None),
        RoutingMode::ConnectionId,
        worker_count,
    );
    let target_partition = (initial_partition + 1) % worker_count;
    let registry = load_or_create_worker_registry(&data_dir, worker_count)?;
    let target_worker = registry.worker(target_partition).unwrap();

    let (stop_notifier, server_thread) =
        spawn_iouring_server(port, worker_count, &data_dir, 64 * 1024 * 1024);
    wait_until_server_ready_async(port).await;

    {
        let mut client = AsyncPerfClient::connect(port).await?;
        let original_session_id = client.session_id;
        let rebound_session_id = client
            .create_session(Some(
                serde_json::json!({
                    "session_id": original_session_id.to_string(),
                    "worker_id": target_worker.worker_id.to_string(),
                })
                .to_string(),
            ))
            .await?;
        assert_eq!(rebound_session_id, original_session_id);
        client
            .put(b"rebind-key".to_vec(), b"rebind-val".to_vec())
            .await?;
        assert_eq!(
            client.get(b"rebind-key".to_vec()).await?,
            Some(b"rebind-val".to_vec())
        );
    }

    stop_notifier.notify_all();
    server_thread.join().unwrap()?;
    Ok(())
}
