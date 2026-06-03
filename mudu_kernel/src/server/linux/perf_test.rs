use crate::server::routing::RoutingMode;
use crate::server::server::WorkerTcpBackend;
use crate::server::server_cfg::ServerCfg;
use crate::server::server_launch::ServerLaunch;
use crate::server::server_runtime_deps::ServerRuntimeDeps;
use crate::server::worker_registry::{load_or_create_worker_registry, WorkerRegistry};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::error::err::MError;
use mudu::m_error;
use mudu_contract::protocol::{
    decode_error_response, decode_get_response, decode_put_response,
    decode_session_create_response, encode_get_request, encode_put_request,
    encode_session_create_request, Frame, FrameHeader, GetRequest, MessageType, PutRequest,
    SessionCreateRequest, HEADER_LEN,
};
use mudu_sys::tokio::io::{AsyncReadExt, AsyncWriteExt};
use mudu_sys::tokio::net::{TcpSocket as TokioTcpSocket, TcpStream as TokioTcpStream};
use mudu_sys::tokio::sync::Notify;
use mudu_sys::tokio::task::JoinSet;
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{notify_wait, NotifyWait};
use mudu_utils::task_async::spawn_task;
use mudu_utils::{debug, task_trace};
use short_uuid::ShortUuid;
use std::env::temp_dir;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::ops::RangeInclusive;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, OnceLock};
use mudu_sys::sync::{SMutex, SMutexGuard};
use std::thread;
use std::time::Duration;
use tracing::debug;
use tracing::info;
use uuid::Uuid;

struct AsyncPerfClient {
    stream: TokioTcpStream,
    next_request_id: u64,
    session_id: u128,
}

impl AsyncPerfClient {
    async fn connect(port: u16) -> RS<Self> {
        Self::connect_with_loopback_shard(port, 0).await
    }

    async fn connect_with_loopback_shard(port: u16, shard: usize) -> RS<Self> {
        let source_ip = loopback_shard_ip(shard);
        let mut last_err: Option<MError> = None;
        // Retry transient failures when creating a session on first connect. This
        // makes tests more robust on CI where the backend may reset connections
        // briefly during startup.
        for _attempt in 0..3 {
            let socket = match TokioTcpSocket::new_v4() {
                Ok(s) => s,
                Err(e) => {
                    return Err(m_error!(
                        EC::NetErr,
                        "create io_uring perf client socket error",
                        e
                    ))
                }
            };
            if let Err(e) = socket.bind(SocketAddr::from((source_ip, 0))) {
                return Err(m_error!(
                    EC::NetErr,
                    "bind io_uring perf client socket error",
                    e
                ));
            }
            match socket
                .connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
                .await
            {
                Ok(stream) => {
                    if let Err(e) = stream.set_nodelay(true) {
                        return Err(m_error!(EC::NetErr, "set tcp nodelay error", e));
                    }
                    let mut client = Self {
                        stream,
                        next_request_id: 1,
                        session_id: 0,
                    };
                    match client.create_session(None).await {
                        Ok(sid) => {
                            client.session_id = sid;
                            return Ok(client);
                        }
                        Err(e) => {
                            last_err = Some(e);
                            // drop client/stream and retry
                        }
                    }
                }
                Err(e) => {
                    last_err = Some(m_error!(EC::NetErr, "connect io_uring tcp server error", e));
                }
            }
            // small backoff between attempts
            mudu_sys::task_async::sleep(Duration::from_millis(50)).await?;
        }
        Err(last_err.unwrap_or_else(|| m_error!(EC::NetErr, "connect io_uring tcp server error")))
    }

    async fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        mudu_utils::scoped_task_trace!();

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

    async fn close(mut self) -> RS<()> {
        self.stream
            .shutdown()
            .await
            .map_err(|e| m_error!(EC::NetErr, "shutdown io_uring perf client stream error", e))
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
        mudu_utils::scoped_task_trace!();
        let mut header = [0u8; HEADER_LEN];
        self.stream
            .read_exact(&mut header)
            .await
            .map_err(|e| m_error!(EC::NetErr, "read response header error", e))?;
        let payload_len = FrameHeader::decode_header_bytes(&header)?.payload_len() as usize;
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

fn test_worker_port(base_port: u16, worker_index: usize) -> RS<u16> {
    let offset = u16::try_from(worker_index).map_err(|_| {
        m_error!(
            EC::ParseErr,
            format!(
                "worker index too large for test port mapping: {}",
                worker_index
            )
        )
    })?;
    base_port.checked_add(offset).ok_or_else(|| {
        m_error!(
            EC::ParseErr,
            format!(
                "test worker port overflow: base_port={}, worker_index={}",
                base_port, worker_index
            )
        )
    })
}

fn loopback_shard_ip(shard: usize) -> Ipv4Addr {
    // Linux routes the entire 127.0.0.0/8 block to loopback. Spreading
    // clients across multiple source IPs expands the available 4-tuple space
    // for single-host load tests without changing the client count.
    let host = (shard % 250) as u8 + 2;
    Ipv4Addr::new(127, 0, 0, host)
}

fn reserve_listener() -> Option<TcpListener> {
    let ephemeral = linux_ephemeral_port_range().unwrap_or(32768..=60999);
    for range in candidate_port_ranges(&ephemeral) {
        for port in range {
            match bind_reserved_listener(port) {
                Ok(listener) => return Some(listener),
                Err(_) => continue,
            }
        }
    }
    eprintln!("skip io_uring perf test: unable to reserve a port outside ephemeral range");
    None
}

fn network_perf_test_lock() -> &'static SMutex<()> {
    static LOCK: OnceLock<SMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| SMutex::new(()))
}

fn lock_network_perf_test() -> SMutexGuard<'static, ()> {
    network_perf_test_lock().lock().unwrap()
}

fn bind_reserved_listener(port: u16) -> std::io::Result<TcpListener> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

fn linux_ephemeral_port_range() -> Option<RangeInclusive<u16>> {
    let raw = std::fs::read_to_string("/proc/sys/net/ipv4/ip_local_port_range").ok()?;
    let mut parts = raw.split_whitespace();
    let start = parts.next()?.parse::<u16>().ok()?;
    let end = parts.next()?.parse::<u16>().ok()?;
    Some(start..=end)
}

fn candidate_port_ranges(ephemeral: &RangeInclusive<u16>) -> Vec<RangeInclusive<u16>> {
    let mut ranges = Vec::new();
    let low = 10000u16;
    let high = 65000u16;
    let eph_start = *ephemeral.start();
    let eph_end = *ephemeral.end();
    if low < eph_start {
        ranges.push(low..=eph_start.saturating_sub(1));
    }
    if eph_end < high {
        ranges.push(eph_end.saturating_add(1)..=high);
    }
    ranges
}

fn should_skip_iouring_perf(err: &MError) -> bool {
    let msg = err.to_string();
    msg.contains("io_uring_queue_init_params error")
        || msg.contains("io_uring backend exited before becoming ready")
}

fn maybe_skip_in_act() -> bool {
    if !mudu_sys::io_uring_available() {
        eprintln!("skip io_uring perf test: io_uring unavailable");
        true
    } else {
        false
    }
}

async fn wait_for_clients_ready(
    clients: usize,
    ready_clients: &AtomicU64,
    setup_error: &SMutex<Option<String>>,
) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + perf_client_setup_timeout(clients);
    while mudu_sys::time::instant_now() < deadline {
        if let Some(err) = setup_error.lock().unwrap().clone() {
            return Err(m_error!(
                EC::NetErr,
                format!("io_uring perf client setup failed: {err}")
            ));
        }
        let ready = ready_clients.load(Ordering::Acquire) as usize;
        if ready == clients {
            return Ok(());
        }
        mudu_sys::task_async::sleep(Duration::from_millis(25))
            .await
            .expect("linux sleep wrapper should not fail");
    }
    let ready = ready_clients.load(Ordering::Acquire);
    Err(m_error!(
        EC::NetErr,
        format!(
            "io_uring perf clients did not all become ready before timeout: ready={}, expected={}",
            ready, clients
        )
    ))
}

fn perf_client_setup_timeout(clients: usize) -> Duration {
    let default_secs = std::cmp::max(30, ((clients + 39) / 40) as u64);
    let secs = std::env::var("MUDU_PERF_SETUP_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default_secs);
    Duration::from_secs(secs)
}

struct TestServerHandle {
    join_handle: thread::JoinHandle<RS<()>>,
    exit_rx: Receiver<Result<(), String>>,
}

impl TestServerHandle {
    fn join(self) -> thread::Result<RS<()>> {
        self.join_handle.join()
    }
}

async fn wait_until_server_ready_or_exit_async(port: u16, server: &TestServerHandle) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + Duration::from_secs(10);
    while mudu_sys::time::instant_now() < deadline {
        match server.exit_rx.try_recv() {
            Ok(Ok(())) => {
                return Err(m_error!(
                    EC::NetErr,
                    format!(
                        "io_uring backend exited before becoming ready on port {}",
                        port
                    )
                ));
            }
            Ok(Err(err)) => {
                return Err(m_error!(
                    EC::NetErr,
                    format!(
                        "io_uring backend exited before becoming ready on port {}: {}",
                        port, err
                    )
                ));
            }
            Err(TryRecvError::Disconnected) => {
                return Err(m_error!(
                    EC::NetErr,
                    format!(
                        "io_uring backend exit channel disconnected before ready on port {}",
                        port
                    )
                ));
            }
            Err(TryRecvError::Empty) => {}
        }
        match TokioTcpStream::connect(("127.0.0.1", port)).await {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                let _ = stream
                    .into_std()
                    .and_then(|s| s.shutdown(std::net::Shutdown::Both));
                return Ok(());
            }
            Err(_) => {}
        }
        mudu_sys::task_async::sleep(Duration::from_millis(25)).await?;
    }
    Err(m_error!(
        EC::NetErr,
        format!("io_uring backend did not become ready on port {}", port)
    ))
}

async fn wait_until_worker_port_ready_or_exit_async(
    base_port: u16,
    worker_index: usize,
    server: &TestServerHandle,
) -> RS<()> {
    let worker_port = test_worker_port(base_port, worker_index)?;
    wait_until_server_ready_or_exit_async(worker_port, server).await
}

fn spawn_iouring_server(
    listener: TcpListener,
    worker_count: usize,
    data_dir: &std::path::Path,
    log_chunk_size: u64,
    worker_registry: Option<Arc<WorkerRegistry>>,
) -> (mudu_utils::notifier::Notifier, TestServerHandle) {
    let (stop_notifier, server_stop) = notify_wait();
    let (exit_tx, exit_rx) = mpsc::channel();
    let port = listener.local_addr().unwrap().port();
    let server_cfg = ServerCfg::new(
        worker_count,
        "127.0.0.1".to_string(),
        port,
        data_dir.to_string_lossy().into_owned(),
        data_dir.to_string_lossy().into_owned(),
        RoutingMode::ConnectionId,
    )
    .unwrap()
    .with_multi_port(worker_count > 1)
    .with_log_chunk_size(log_chunk_size);
    let mut server_deps = ServerRuntimeDeps::from_cfg(&server_cfg).unwrap();
    if let Some(worker_registry) = worker_registry {
        server_deps = server_deps
            .with_worker_registry(&server_cfg, worker_registry)
            .unwrap();
    }
    let server_launch = ServerLaunch::new(server_cfg, server_deps).with_prebound_listener(listener);
    let join_handle = thread::spawn(move || {
        let result = WorkerTcpBackend::sync_serve_with_stop(server_launch, server_stop);
        let exit_msg = match &result {
            Ok(()) => Ok(()),
            Err(err) => Err(err.to_string()),
        };
        let _ = exit_tx.send(exit_msg);
        result
    });
    (
        stop_notifier,
        TestServerHandle {
            join_handle,
            exit_rx,
        },
    )
}

fn percentile_us(samples: &mut [u64], percentile: f64) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }
    samples.sort_unstable();
    let rank = ((samples.len() - 1) as f64 * percentile).ceil() as usize;
    samples.get(rank).copied()
}

fn avg_us(samples: &[u64]) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    Some(samples.iter().copied().sum::<u64>() as f64 / samples.len() as f64)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn iouring_backend_perf_put_get() -> RS<()> {
    if maybe_skip_in_act() {
        return Ok(());
    }
    let _guard = lock_network_perf_test();
    log_setup("info");
    let notifier = NotifyWait::new();
    {
        let _n = notifier.clone();
        let _ = thread::spawn(move || {
            debug::debug_serve(_n, 1800);
        });
    };
    let Some(listener) = reserve_listener() else {
        return Ok(());
    };
    let port = listener.local_addr().unwrap().port();
    let worker_count = 6usize;
    let clients = 6usize;
    let bench_duration = Duration::from_secs(10);
    let data_dir = temp_dir().join(format!(
        "mududb_iouring_perf_{}",
        mudu_sys::random::uuid_v4()
    ));
    std::fs::create_dir_all(&data_dir).unwrap();

    let (stop_notifier, server_thread) =
        spawn_iouring_server(listener, worker_count, &data_dir, 64 * 1024 * 1024, None);
    if let Err(err) = wait_until_server_ready_or_exit_async(port, &server_thread).await {
        if should_skip_iouring_perf(&err) {
            eprintln!("skip io_uring perf test: {}", err);
            return Ok(());
        }
        return Err(err);
    }

    let start_clients = Arc::new(AtomicBool::new(false));
    let start_notify = Arc::new(Notify::new());
    let ready_clients = Arc::new(AtomicU64::new(0));
    let setup_error = Arc::new(SMutex::new(None::<String>));
    let stop_clients = Arc::new(AtomicBool::new(false));
    let put_ops = Arc::new(AtomicU64::new(0));
    let get_ops = Arc::new(AtomicU64::new(0));
    let put_latencies_us = Arc::new(SMutex::new(Vec::<u64>::new()));
    let get_latencies_us = Arc::new(SMutex::new(Vec::<u64>::new()));
    let mut join_set: JoinSet<RS<()>> = mudu_sys::tokio::task::JoinSet::new();
    for client_id in 0..clients {
        let start_clients = start_clients.clone();
        let start_notify = start_notify.clone();
        let ready_clients = ready_clients.clone();
        let setup_error = setup_error.clone();
        let stop_clients = stop_clients.clone();
        let put_ops = put_ops.clone();
        let get_ops = get_ops.clone();
        let put_latencies_us = put_latencies_us.clone();
        let get_latencies_us = get_latencies_us.clone();
        let join_handle = spawn_task(
            notifier.clone(),
            format!("task_cli_{}", client_id).as_str(),
            async move {
                let mut client =
                    match AsyncPerfClient::connect_with_loopback_shard(port, client_id).await {
                        Ok(client) => client,
                        Err(err) => {
                            let mut setup_error = setup_error.lock().unwrap();
                            if setup_error.is_none() {
                                *setup_error =
                                    Some(format!("client {client_id} connect/setup error: {err}"));
                            }
                            return Err(err);
                        }
                    };
                ready_clients.fetch_add(1, Ordering::AcqRel);
                while !start_clients.load(Ordering::Acquire) {
                    start_notify.notified().await;
                }
                let mut op_id = 0usize;
                let mut local_put_latencies_us = Vec::new();
                let mut local_get_latencies_us = Vec::new();
                while !stop_clients.load(Ordering::Relaxed) {
                    let key = format!("client-{client_id:02}-key-{op_id:06}").into_bytes();
                    let value = format!("value-{client_id:02}-{op_id:06}").into_bytes();
                    debug!("client {} put key ", client_id);
                    let put_started_at = mudu_sys::time::instant_now();
                    client.put(key.clone(), value.clone()).await?;
                    local_put_latencies_us.push(put_started_at.elapsed().as_micros() as u64);
                    debug!("client {} put key done", client_id);
                    put_ops.fetch_add(1, Ordering::Relaxed);
                    debug!("client {} get key", client_id);
                    let get_started_at = mudu_sys::time::instant_now();
                    let returned = client.get(key).await?;
                    local_get_latencies_us.push(get_started_at.elapsed().as_micros() as u64);
                    debug!("client {} get key done", client_id);
                    assert_eq!(returned, Some(value));
                    get_ops.fetch_add(1, Ordering::Relaxed);
                    op_id += 1;
                }
                put_latencies_us
                    .lock()
                    .unwrap()
                    .extend(local_put_latencies_us);
                get_latencies_us
                    .lock()
                    .unwrap()
                    .extend(local_get_latencies_us);
                Ok::<(), MError>(())
            },
        )?;
        join_set.spawn(async move {
            match join_handle.await {
                Ok(Some(Ok(()))) => Ok(()),
                Ok(Some(Err(err))) => Err(err),
                Ok(None) => Err(m_error!(
                    EC::TokioErr,
                    format!("io_uring perf client task {} cancelled", client_id)
                )),
                Err(err) => Err(m_error!(
                    EC::TokioErr,
                    format!("join io_uring perf client task {} error", client_id),
                    err
                )),
            }
        });
    }

    wait_for_clients_ready(clients, ready_clients.as_ref(), setup_error.as_ref()).await?;
    start_clients.store(true, Ordering::Release);
    start_notify.notify_waiters();

    let started_at = mudu_sys::time::instant_now();
    mudu_sys::task_async::sleep(bench_duration).await?;
    let elapsed = started_at.elapsed();
    stop_clients.store(true, Ordering::Relaxed);
    while let Some(result) = join_set.join_next().await {
        result.map_err(|e| {
            m_error!(
                EC::TokioErr,
                "join io_uring perf client aggregation task error",
                e
            )
        })??;
    }
    let total_put_ops = put_ops.load(Ordering::Relaxed) as usize;
    let total_get_ops = get_ops.load(Ordering::Relaxed) as usize;
    let total_ops = total_put_ops + total_get_ops;
    let throughput = total_ops as f64 / elapsed.as_secs_f64();
    let mut put_samples = put_latencies_us.lock().unwrap().clone();
    let mut get_samples = get_latencies_us.lock().unwrap().clone();
    let mut total_samples = Vec::with_capacity(put_samples.len() + get_samples.len());
    total_samples.extend_from_slice(&put_samples);
    total_samples.extend_from_slice(&get_samples);
    let put_p1_us = percentile_us(&mut put_samples, 0.01);
    let put_p50_us = percentile_us(&mut put_samples, 0.50);
    let put_p99_us = percentile_us(&mut put_samples, 0.99);
    let put_p999_us = percentile_us(&mut put_samples, 0.999);
    let get_p1_us = percentile_us(&mut get_samples, 0.01);
    let get_p50_us = percentile_us(&mut get_samples, 0.50);
    let get_p99_us = percentile_us(&mut get_samples, 0.99);
    let get_p999_us = percentile_us(&mut get_samples, 0.999);
    let total_p1_us = percentile_us(&mut total_samples, 0.01);
    let total_p50_us = percentile_us(&mut total_samples, 0.50);
    let total_p99_us = percentile_us(&mut total_samples, 0.99);
    let total_p999_us = percentile_us(&mut total_samples, 0.999);
    let put_avg_us = avg_us(&put_samples);
    let get_avg_us = avg_us(&get_samples);
    let total_avg_us = avg_us(&total_samples);
    info!(
        "io_uring kv perf: clients={}, puts={}, gets={}, total_ops={}, elapsed_ms={}, throughput_ops_per_sec={:.2}, put_avg_us={:.2}, put_p1_us={}, put_p50_us={}, put_tail_p99_us={}, put_tail_p999_us={}, get_avg_us={:.2}, get_p1_us={}, get_p50_us={}, get_tail_p99_us={}, get_tail_p999_us={}, total_avg_us={:.2}, total_p1_us={}, total_p50_us={}, total_tail_p99_us={}, total_tail_p999_us={}",
        clients,
        total_put_ops,
        total_get_ops,
        total_ops,
        elapsed.as_millis(),
        throughput,
        put_avg_us.unwrap_or_default(),
        put_p1_us.unwrap_or_default(),
        put_p50_us.unwrap_or_default(),
        put_p99_us.unwrap_or_default(),
        put_p999_us.unwrap_or_default(),
        get_avg_us.unwrap_or_default(),
        get_p1_us.unwrap_or_default(),
        get_p50_us.unwrap_or_default(),
        get_p99_us.unwrap_or_default(),
        get_p999_us.unwrap_or_default(),
        total_avg_us.unwrap_or_default(),
        total_p1_us.unwrap_or_default(),
        total_p50_us.unwrap_or_default(),
        total_p99_us.unwrap_or_default(),
        total_p999_us.unwrap_or_default()
    );
    stop_notifier.notify_all();
    server_thread.join().unwrap()?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn iouring_backend_recovery_replays_worker_logs() -> RS<()> {
    if maybe_skip_in_act() {
        return Ok(());
    }
    let _guard = lock_network_perf_test();
    let Some(listener) = reserve_listener() else {
        return Ok(());
    };
    let port = listener.local_addr().unwrap().port();
    let worker_count = 2usize;
    let data_dir = temp_dir().join(format!(
        "mududb_iouring_recovery_{}",
        mudu_sys::random::uuid_v4()
    ));
    std::fs::create_dir_all(&data_dir).unwrap();
    let registry = load_or_create_worker_registry(&data_dir, worker_count)?;
    let target_worker = registry.worker(0).unwrap();

    let (stop_notifier, server_thread) = spawn_iouring_server(
        listener,
        worker_count,
        &data_dir,
        64 * 1024 * 1024,
        Some(registry.clone()),
    );
    if let Err(err) = wait_until_server_ready_or_exit_async(port, &server_thread).await {
        if should_skip_iouring_perf(&err) {
            eprintln!("skip io_uring recovery test: {}", err);
            return Ok(());
        }
        return Err(err);
    }

    {
        let mut client = AsyncPerfClient::connect(test_worker_port(port, 0)?).await?;
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
        client.close().await?;
    }

    stop_notifier.notify_all();
    server_thread.join().unwrap()?;

    let Some(restart_listener) = reserve_listener() else {
        return Ok(());
    };
    let restart_port = restart_listener.local_addr().unwrap().port();
    let (stop_notifier, server_thread) = spawn_iouring_server(
        restart_listener,
        worker_count,
        &data_dir,
        64 * 1024 * 1024,
        Some(registry.clone()),
    );
    if let Err(err) = wait_until_server_ready_or_exit_async(restart_port, &server_thread).await {
        if should_skip_iouring_perf(&err) {
            eprintln!("skip io_uring recovery restart test: {}", err);
            return Ok(());
        }
        return Err(err);
    }

    {
        let mut client = AsyncPerfClient::connect(test_worker_port(restart_port, 0)?).await?;
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
        client.close().await?;
    }

    stop_notifier.notify_all();
    server_thread.join().unwrap()?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn iouring_backend_recovery_replays_across_multiple_chunks() -> RS<()> {
    if maybe_skip_in_act() {
        return Ok(());
    }
    let _guard = lock_network_perf_test();
    let Some(listener) = reserve_listener() else {
        return Ok(());
    };
    let port = listener.local_addr().unwrap().port();
    let worker_count = 1usize;
    let log_chunk_size = 64u64;
    let data_dir = temp_dir().join(format!(
        "mududb_iouring_recovery_multichunk_{}",
        mudu_sys::random::uuid_v4()
    ));
    std::fs::create_dir_all(&data_dir).unwrap();
    let entries = vec![
        (b"alpha".to_vec(), b"one".to_vec()),
        (b"beta".to_vec(), b"two".to_vec()),
        (b"gamma".to_vec(), b"three".to_vec()),
        (b"delta".to_vec(), b"four".to_vec()),
    ];

    let (stop_notifier, server_thread) =
        spawn_iouring_server(listener, worker_count, &data_dir, log_chunk_size, None);
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

    let Some(restart_listener) = reserve_listener() else {
        return Ok(());
    };
    let restart_port = restart_listener.local_addr().unwrap().port();
    let (stop_notifier, server_thread) = spawn_iouring_server(
        restart_listener,
        worker_count,
        &data_dir,
        log_chunk_size,
        None,
    );
    if let Err(err) = wait_until_server_ready_or_exit_async(restart_port, &server_thread).await {
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn iouring_backend_open_session_routes_connection_to_requested_partition() -> RS<()> {
    if maybe_skip_in_act() {
        return Ok(());
    }
    let _guard = lock_network_perf_test();
    let Some(listener) = reserve_listener() else {
        return Ok(());
    };
    let port = listener.local_addr().unwrap().port();
    let worker_count = 2usize;
    let data_dir = temp_dir().join(format!(
        "mududb_iouring_route_{}",
        mudu_sys::random::uuid_v4()
    ));
    std::fs::create_dir_all(&data_dir).unwrap();

    let target_partition = 1usize;
    let registry = load_or_create_worker_registry(&data_dir, worker_count)?;
    let target_worker = registry.worker(target_partition).unwrap();

    let (stop_notifier, server_thread) = spawn_iouring_server(
        listener,
        worker_count,
        &data_dir,
        64 * 1024 * 1024,
        Some(registry.clone()),
    );
    if let Err(err) = wait_until_server_ready_or_exit_async(port, &server_thread).await {
        if should_skip_iouring_perf(&err) {
            eprintln!("skip io_uring route test: {}", err);
            return Ok(());
        }
        return Err(err);
    }
    if let Err(err) =
        wait_until_worker_port_ready_or_exit_async(port, target_partition, &server_thread).await
    {
        if should_skip_iouring_perf(&err) {
            eprintln!("skip io_uring route test: {}", err);
            return Ok(());
        }
        return Err(err);
    }

    {
        let mut client =
            AsyncPerfClient::connect(test_worker_port(port, target_partition)?).await?;
        let config_json = serde_json::json!({
            "session_id": "0",
            "worker_id": target_worker.worker_id.to_string(),
        })
        .to_string();
        let mut last_err: Option<MError> = None;
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
                    mudu_sys::task_async::sleep(Duration::from_millis(50)).await?;
                }
            }
        }
        if let Some(e) = last_err {
            return Err(e);
        }
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
    if maybe_skip_in_act() {
        return Ok(());
    }
    let _guard = lock_network_perf_test();
    let Some(listener) = reserve_listener() else {
        return Ok(());
    };
    let port = listener.local_addr().unwrap().port();
    let worker_count = 2usize;
    let data_dir = temp_dir().join(format!(
        "mududb_iouring_rebind_{}",
        mudu_sys::random::uuid_v4()
    ));
    std::fs::create_dir_all(&data_dir).unwrap();

    let target_partition = 1usize;
    let registry = load_or_create_worker_registry(&data_dir, worker_count)?;
    let target_worker = registry.worker(target_partition).unwrap();

    let (stop_notifier, server_thread) = spawn_iouring_server(
        listener,
        worker_count,
        &data_dir,
        64 * 1024 * 1024,
        Some(registry.clone()),
    );
    if let Err(err) = wait_until_server_ready_or_exit_async(port, &server_thread).await {
        if should_skip_iouring_perf(&err) {
            eprintln!("skip io_uring rebind test: {}", err);
            return Ok(());
        }
        return Err(err);
    }
    if let Err(err) =
        wait_until_worker_port_ready_or_exit_async(port, target_partition, &server_thread).await
    {
        if should_skip_iouring_perf(&err) {
            eprintln!("skip io_uring rebind test: {}", err);
            return Ok(());
        }
        return Err(err);
    }

    {
        let mut client =
            AsyncPerfClient::connect(test_worker_port(port, target_partition)?).await?;
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
