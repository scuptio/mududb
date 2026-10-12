#![allow(clippy::await_holding_lock)]

use crate::routing::RoutingMode;
use crate::server::WorkerTcpBackend;
use crate::server_cfg::ServerCfg;
use crate::server_launch::ServerLaunch;
use crate::server_runtime_deps::ServerRuntimeDeps;
use crate::worker_registry::{load_or_create_worker_registry, WorkerRegistry};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use mudu_contract::protocol::{
    decode_error_response, decode_get_response, decode_put_response,
    decode_session_create_response, encode_get_request, encode_put_request,
    encode_session_create_request, Frame, FrameHeader, GetRequest, MessageType, PutRequest,
    SessionCreateRequest, HEADER_LEN,
};
use mudu_sys::env_var::temp_dir;
use mudu_sys::net::sync::StdTcpListener;
use mudu_sys::net::AsyncTcpStream;
use mudu_sys::sync::SMutex;
use mudu_sys::sync::SMutexGuard;
use mudu_sys::task::sync::SJoinHandle;
use mudu_sys::tokio::io::{AsyncReadExt, AsyncWriteExt};
use mudu_sys::tokio::net::TcpSocket as TokioTcpSocket;
use mudu_sys::tokio::sync::Notify;
use mudu_sys::tokio::task::JoinSet;
use mudu_utils::debug::debug_serve;
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{notify_wait, NotifyWait};
use mudu_utils::task_async::spawn_task;
use mudu_utils::task_trace;
use short_uuid::ShortUuid;
use std::net::{Ipv4Addr, SocketAddr};
use std::ops::RangeInclusive;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;
use tracing::debug;
use tracing::info;
use uuid::Uuid;

struct AsyncPerfClient {
    stream: AsyncTcpStream,
    next_request_id: u64,
    session_id: u128,
}

impl AsyncPerfClient {
    async fn connect(port: u16) -> RS<Self> {
        Self::connect_with_loopback_shard(port, 0).await
    }

    async fn connect_with_loopback_shard(port: u16, shard: usize) -> RS<Self> {
        let trace = task_trace!();
        trace.watch("perf_client.op", "connect");
        trace.watch("perf_client.port", &port.to_string());
        trace.watch("perf_client.shard", &shard.to_string());
        debug!("Connecting to {}", port);
        let source_ip = loopback_shard_ip(shard);
        let mut last_err: Option<MuduError> = None;
        // Retry transient failures when creating a session on first connect. This
        // makes tests more robust on CI where the backend may reset connections
        // briefly during startup.
        for _attempt in 0..3 {
            trace.watch("perf_client.attempt", &_attempt.to_string());
            trace.watch("perf_client.stage", "socket_new");
            debug!("Connecting to {}, attempt {}", port, _attempt);
            let socket = match TokioTcpSocket::new_v4() {
                Ok(s) => s,
                Err(e) => {
                    return Err(mudu_error!(
                        ErrorCode::Network,
                        "create io_uring perf client socket error",
                        e
                    ));
                }
            };
            trace.watch("perf_client.stage", "bind");
            if let Err(e) = socket.bind(SocketAddr::from((source_ip, 0))) {
                return Err(mudu_error!(
                    ErrorCode::Network,
                    "bind io_uring perf client socket error",
                    e
                ));
            }
            trace.watch("perf_client.stage", "connect");
            match socket
                .connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
                .await
            {
                Ok(stream) => {
                    trace.watch("perf_client.stage", "create_initial_session");
                    let stream = AsyncTcpStream::new(
                        mudu_sys::imp::net::async_tokio::TokioTcpStream::new(stream),
                    );
                    if let Err(e) = stream.set_nodelay(true) {
                        return Err(mudu_error!(ErrorCode::Network, "set tcp nodelay error", e));
                    }
                    let mut client = Self {
                        stream,
                        next_request_id: 1,
                        session_id: 0,
                    };
                    match client.create_session(None).await {
                        Ok(sid) => {
                            trace.watch("perf_client.stage", "connected");
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
                    last_err = Some(mudu_error!(
                        ErrorCode::Network,
                        "connect io_uring tcp server error",
                        e
                    ));
                }
            }
            // small backoff between attempts
            mudu_sys::task::async_::sleep(Duration::from_millis(50)).await?;
        }
        Err(last_err.unwrap_or_else(|| {
            mudu_error!(ErrorCode::Network, "connect io_uring tcp server error")
        }))
    }

    async fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        let trace = task_trace!();
        trace.watch("perf_client.op", "put");
        trace.watch("perf_client.session_id", &self.session_id.to_string());

        let request_id = self.take_request_id();
        trace.watch("perf_client.request_id", &request_id.to_string());
        let payload =
            encode_put_request(request_id, &PutRequest::new(self.session_id, key, value))?;
        let frame = self.send_and_receive(&payload).await?;
        self.ensure_success_frame(&frame)?;
        if decode_put_response(&frame)?.ok() {
            Ok(())
        } else {
            Err(mudu_error!(
                ErrorCode::Network,
                "remote put operation returned failure"
            ))
        }
    }

    async fn get(&mut self, key: Vec<u8>) -> RS<Option<Vec<u8>>> {
        let trace = task_trace!();
        trace.watch("perf_client.op", "get");
        trace.watch("perf_client.session_id", &self.session_id.to_string());
        let request_id = self.take_request_id();
        trace.watch("perf_client.request_id", &request_id.to_string());
        let payload = encode_get_request(request_id, &GetRequest::new(self.session_id, key))?;
        let frame = self.send_and_receive(&payload).await?;
        self.ensure_success_frame(&frame)?;
        Ok(decode_get_response(&frame)?.into_value())
    }

    async fn create_session(&mut self, config_json: Option<String>) -> RS<u128> {
        let trace = task_trace!();
        trace.watch("perf_client.op", "create_session");
        trace.watch(
            "perf_client.has_config",
            if config_json.is_some() {
                "true"
            } else {
                "false"
            },
        );
        let request_id = self.take_request_id();
        trace.watch("perf_client.request_id", &request_id.to_string());
        let payload =
            encode_session_create_request(request_id, &SessionCreateRequest::new(config_json))?;
        let frame = self.send_and_receive(&payload).await?;
        self.ensure_success_frame(&frame)?;
        Ok(decode_session_create_response(&frame)?.session_id())
    }

    async fn close(mut self) -> RS<()> {
        self.stream.shutdown().await.map_err(|e| {
            mudu_error!(
                ErrorCode::Network,
                "shutdown io_uring perf client stream error",
                e
            )
        })
    }

    fn take_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        request_id
    }

    async fn send_and_receive(&mut self, payload: &[u8]) -> RS<Frame> {
        let trace = task_trace!();
        trace.watch("perf_client.io", "send");
        trace.watch("perf_client.payload_len", &payload.len().to_string());
        self._send(payload).await?;
        trace.watch("perf_client.io", "receive");
        self._receive().await
    }

    async fn _send(&mut self, payload: &[u8]) -> RS<()> {
        let trace = task_trace!();
        trace.watch("perf_client.io", "write_all");
        self.stream
            .write_all(payload)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, "write request frame error", e))?;
        trace.watch("perf_client.io", "flush");
        self.stream
            .flush()
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, "flush request frame error", e))?;
        Ok(())
    }
    async fn _receive(&mut self) -> RS<Frame> {
        let trace = task_trace!();
        trace.watch("perf_client.io", "read_header");
        let mut header = [0u8; HEADER_LEN];
        self.stream
            .read_exact(&mut header)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, "read response header error", e))?;
        let payload_len = FrameHeader::decode_header_bytes(&header)?.payload_len() as usize;
        trace.watch("perf_client.io", "read_body");
        trace.watch("perf_client.response_payload_len", &payload_len.to_string());
        let mut frame_bytes = Vec::with_capacity(HEADER_LEN + payload_len);
        frame_bytes.extend_from_slice(&header);
        if payload_len > 0 {
            let mut body = vec![0u8; payload_len];
            self.stream
                .read_exact(&mut body)
                .await
                .map_err(|e| mudu_error!(ErrorCode::Network, "read response payload error", e))?;
            frame_bytes.extend_from_slice(&body);
        }
        trace.watch("perf_client.io", "decode_frame");
        Frame::decode(&frame_bytes)
    }

    fn ensure_success_frame(&self, frame: &Frame) -> RS<()> {
        let _trace = task_trace!();
        if frame.header().message_type() == MessageType::Error {
            let error = decode_error_response(frame)?;
            return Err(mudu_error!(ErrorCode::Network, error.message()));
        }
        Ok(())
    }
}

fn test_worker_port(base_port: u16, worker_index: usize) -> RS<u16> {
    let offset = u16::try_from(worker_index).map_err(|_| {
        mudu_error!(
            ErrorCode::Parse,
            format!(
                "worker index too large for test port mapping: {}",
                worker_index
            )
        )
    })?;
    base_port.checked_add(offset).ok_or_else(|| {
        mudu_error!(
            ErrorCode::Parse,
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

/// Reserves a contiguous block of `count` TCP ports starting outside the
/// ephemeral range. Each worker in multi-port mode binds a distinct port, so
/// reserving the whole block up front prevents `Address already in use` races
/// where worker 0 obtains a free base port but worker 1's offset port is taken.
fn reserve_listener_block(count: usize) -> Option<Vec<StdTcpListener>> {
    if count == 0 {
        return Some(Vec::new());
    }
    let ephemeral = linux_ephemeral_port_range().unwrap_or(32768..=60999);
    for range in candidate_port_ranges(&ephemeral) {
        for base in range {
            if base
                .checked_add(u16::try_from(count).unwrap_or(u16::MAX).saturating_sub(1))
                .is_none()
            {
                continue;
            }
            let mut listeners = Vec::with_capacity(count);
            let mut ok = true;
            for offset in 0..count {
                let port = base + u16::try_from(offset).expect("offset fits in u16");
                match bind_reserved_listener(port) {
                    Ok(listener) => listeners.push(listener),
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                return Some(listeners);
            }
            // Ports in this block are not all free; drop the partially reserved
            // listeners and try the next base port.
        }
    }
    eprintln!(
        "skip io_uring perf test: unable to reserve {} contiguous ports outside ephemeral range",
        count
    );
    None
}

fn network_perf_test_lock() -> &'static SMutex<()> {
    static LOCK: OnceLock<SMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| SMutex::new(()))
}

fn lock_network_perf_test() -> SMutexGuard<'static, ()> {
    network_perf_test_lock().lock().unwrap()
}

fn bind_reserved_listener(port: u16) -> RS<StdTcpListener> {
    let listener = StdTcpListener::bind(format!("127.0.0.1:{}", port).parse().unwrap())?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

fn linux_ephemeral_port_range() -> Option<RangeInclusive<u16>> {
    let raw = mudu_sys::fs::sync::read_to_string("/proc/sys/net/ipv4/ip_local_port_range").ok()?;
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

fn should_skip_iouring_perf(err: &MuduError) -> bool {
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
            return Err(mudu_error!(
                ErrorCode::Network,
                format!("io_uring perf client setup failed: {err}")
            ));
        }
        let ready = ready_clients.load(Ordering::Acquire) as usize;
        if ready == clients {
            return Ok(());
        }
        mudu_sys::task::async_::sleep(Duration::from_millis(25))
            .await
            .expect("linux sleep wrapper should not fail");
    }
    let ready = ready_clients.load(Ordering::Acquire);
    Err(mudu_error!(
        ErrorCode::Network,
        format!(
            "io_uring perf clients did not all become ready before timeout: ready={}, expected={}",
            ready, clients
        )
    ))
}

fn perf_client_setup_timeout(clients: usize) -> Duration {
    let default_secs = std::cmp::max(30, clients.div_ceil(40) as u64);
    let secs = mudu_sys::env_var::var("MUDU_PERF_SETUP_TIMEOUT_SECS")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default_secs);
    Duration::from_secs(secs)
}

struct TestServerHandle {
    join_handle: SJoinHandle<RS<()>>,
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
                return Err(mudu_error!(
                    ErrorCode::Network,
                    format!(
                        "io_uring backend exited before becoming ready on port {}",
                        port
                    )
                ));
            }
            Ok(Err(err)) => {
                return Err(mudu_error!(
                    ErrorCode::Network,
                    format!(
                        "io_uring backend exited before becoming ready on port {}: {}",
                        port, err
                    )
                ));
            }
            Err(TryRecvError::Disconnected) => {
                return Err(mudu_error!(
                    ErrorCode::Network,
                    format!(
                        "io_uring backend exit channel disconnected before ready on port {}",
                        port
                    )
                ));
            }
            Err(TryRecvError::Empty) => {}
        }
        if let Ok(stream) = AsyncTcpStream::connect(("127.0.0.1", port)).await {
            let _ = stream.set_nodelay(true);
            let _ = stream
                .into_std()
                .and_then(|s| s.shutdown(std::net::Shutdown::Both));
            return Ok(());
        }
        mudu_sys::task::async_::sleep(Duration::from_millis(25)).await?;
    }
    Err(mudu_error!(
        ErrorCode::Network,
        format!("io_uring backend did not become ready on port {}", port)
    ))
}

/// Waits for the io_uring backend to accept connections on `port`. Returns
/// `Ok(true)` when the backend cannot run in this environment and the test
/// must skip, `Ok(false)` once the port is ready.
async fn wait_ready_or_skip_io_uring(
    port: u16,
    server: &TestServerHandle,
    skip_msg: &str,
) -> RS<bool> {
    if let Err(err) = wait_until_server_ready_or_exit_async(port, server).await {
        if should_skip_iouring_perf(&err) {
            eprintln!("{}: {}", skip_msg, err);
            return Ok(true);
        }
        return Err(err);
    }
    Ok(false)
}

fn spawn_iouring_server(
    listeners: Vec<StdTcpListener>,
    worker_count: usize,
    data_dir: &std::path::Path,
    log_chunk_size: u64,
    worker_registry: Option<Arc<WorkerRegistry>>,
) -> (mudu_utils::notifier::Notifier, TestServerHandle) {
    let (stop_notifier, server_stop) = notify_wait();
    let (exit_tx, exit_rx) = mpsc::channel();
    let port = listeners
        .first()
        .expect("at least one listener required")
        .local_addr()
        .unwrap()
        .port();
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
    let server_launch =
        ServerLaunch::new(server_cfg, server_deps).with_prebound_listeners(listeners);
    let join_handle = mudu_sys::task::sync::spawn_thread(move || {
        let result = WorkerTcpBackend::sync_serve_with_stop(server_launch, server_stop);
        let exit_msg = match &result {
            Ok(()) => Ok(()),
            Err(err) => Err(err.to_string()),
        };
        let _ = exit_tx.send(exit_msg);
        result
    })
    .unwrap();
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

#[path = "perf_test/put_get.rs"]
mod put_get;
#[path = "perf_test/recovery.rs"]
mod recovery;
#[path = "perf_test/session_routing.rs"]
mod session_routing;
