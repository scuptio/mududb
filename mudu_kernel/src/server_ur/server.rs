use crate::server_ur::procedure_runtime::ProcInvokerPtr;
use crate::server_ur::routing::{
    parse_session_open_config, ConnectionTransfer, RoutingMode, SessionOpenTransferAction,
};
use crate::server_ur::worker::IoUringWorker;
use crate::server_ur::worker_local::WorkerLocal;
use crate::server_ur::worker_registry::{load_or_create_worker_registry, WorkerRegistry};
use crossbeam_queue::SegQueue;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::protocol::{
    decode_client_request, decode_get_request, decode_procedure_invoke_request, decode_put_request,
    decode_range_scan_request, decode_session_close_request, decode_session_create_request,
    encode_error_response, encode_get_response, encode_procedure_invoke_response,
    encode_put_response, encode_range_scan_response, encode_server_response,
    encode_session_close_response, encode_session_create_response, Frame, GetResponse, KeyValue,
    MessageType, ProcedureInvokeResponse, PutResponse, RangeScanResponse, ServerResponse,
    SessionCloseResponse, SessionCreateResponse, HEADER_LEN,
};
use mudu_utils::notifier::{notify_wait, Waiter};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{atomic::AtomicBool, Arc};
use std::thread;
use std::time::Duration;

/// Configuration shared by both execution paths of the `client` backend.
///
/// The `IoUring*` naming is historical and preserved to avoid breaking callers.
/// On Linux this configuration is consumed by the native `io_uring` backend.
/// On non-Linux targets the same configuration is used by a compatible
/// fallback implementation that keeps the worker model and protocol surface
/// unchanged without depending on `io_uring`.
pub struct IoUringTcpServerConfig {
    worker_count: usize,
    listen_ip: String,
    listen_port: u16,
    log_dir: String,
    log_chunk_size: u64,
    routing_mode: RoutingMode,
    procedure_runtime: Option<ProcInvokerPtr>,
    worker_procedure_runtimes: Option<Vec<ProcInvokerPtr>>,
    worker_registry: Arc<WorkerRegistry>,
}

impl IoUringTcpServerConfig {
    /// Creates a backend configuration.
    ///
    /// The resulting value can be used on all supported targets. Linux uses the
    /// native `io_uring` path, while other platforms use the fallback path with
    /// the same externally visible behavior.
    pub fn new(
        worker_count: usize,
        listen_ip: String,
        listen_port: u16,
        log_dir: String,
        routing_mode: RoutingMode,
        procedure_runtime: Option<ProcInvokerPtr>,
    ) -> RS<Self> {
        let worker_registry = load_or_create_worker_registry(&log_dir, worker_count)?;
        Ok(Self {
            worker_count,
            listen_ip,
            listen_port,
            log_dir,
            log_chunk_size: 64 * 1024 * 1024,
            routing_mode,
            procedure_runtime,
            worker_procedure_runtimes: None,
            worker_registry,
        })
    }

    pub fn with_log_chunk_size(mut self, log_chunk_size: u64) -> Self {
        self.log_chunk_size = log_chunk_size;
        self
    }

    /// Installs per-worker procedure runtimes.
    ///
    /// When this is not set, every worker uses `procedure_runtime()`. This hook
    /// exists so upper layers can give each worker an isolated invoker instance
    /// while keeping the transport API unchanged across Linux and non-Linux
    /// implementations.
    pub fn with_worker_procedure_runtimes(mut self, runtimes: Vec<ProcInvokerPtr>) -> Self {
        self.worker_procedure_runtimes = Some(runtimes);
        self
    }

    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub fn listen_ip(&self) -> &str {
        &self.listen_ip
    }

    pub fn listen_port(&self) -> u16 {
        self.listen_port
    }

    pub fn log_dir(&self) -> &str {
        &self.log_dir
    }

    pub fn log_chunk_size(&self) -> u64 {
        self.log_chunk_size
    }

    pub fn routing_mode(&self) -> RoutingMode {
        self.routing_mode
    }

    pub fn worker_registry(&self) -> Arc<WorkerRegistry> {
        self.worker_registry.clone()
    }

    pub fn procedure_runtime(&self) -> Option<ProcInvokerPtr> {
        self.procedure_runtime.clone()
    }

    pub fn procedure_runtime_for_worker(&self, worker_id: usize) -> Option<ProcInvokerPtr> {
        self.worker_procedure_runtimes
            .as_ref()
            .and_then(|runtimes| runtimes.get(worker_id).cloned())
            .or_else(|| self.procedure_runtime())
    }
}

/// Historical backend entry point for the `client` transport.
///
/// The name is preserved for compatibility. Actual behavior is target-specific:
/// Linux runs the native `io_uring` backend, and other platforms run a
/// semantically compatible fallback implementation.
pub struct IoUringTcpBackend;

#[derive(Debug)]
struct TransferredConnection {
    transfer: ConnectionTransfer,
    stream: TcpStream,
    session_ids: Vec<OID>,
    session_open_action: Option<SessionOpenTransferAction>,
}

struct WorkerConnection {
    conn_id: u64,
    state: crate::server_ur::fsm::ConnectionState,
    stream: TcpStream,
    remote_addr: SocketAddr,
    transferred: bool,
    read_buf: Vec<u8>,
    write_buf: Vec<u8>,
}

enum DispatchFrameResult {
    Immediate(Vec<u8>),
    Transfer {
        target_worker: usize,
        session_ids: Vec<OID>,
        action: SessionOpenTransferAction,
    },
}

impl IoUringTcpBackend {
    /// Starts the backend until shutdown.
    ///
    /// This method keeps the old public entry point stable. It dispatches to
    /// the Linux `io_uring` implementation when available and otherwise uses
    /// the portable fallback path.
    pub fn sync_serve(cfg: IoUringTcpServerConfig) -> RS<()> {
        let (_stop_notifier, stop_waiter) = notify_wait();
        Self::sync_serve_with_stop(cfg, stop_waiter)
    }

    /// Internal serve entry that accepts an explicit stop waiter.
    ///
    /// Linux uses `server_iouring`; non-Linux bridges the async stop signal
    /// into an atomic flag and then runs the fallback worker loop.
    pub fn sync_serve_with_stop(cfg: IoUringTcpServerConfig, stop: Waiter) -> RS<()> {
        #[cfg(target_os = "linux")]
        {
            return crate::server_ur::server_iouring::sync_serve_iouring(cfg, stop);
        }

        #[cfg(not(target_os = "linux"))]
        {
            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_for_fallback = stop_flag.clone();
            let notifier = thread::Builder::new()
                .name("iouring-stop-bridge".to_string())
                .spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| {
                            m_error!(
                                EC::TokioErr,
                                "create runtime for io_uring fallback stop bridge error",
                                e
                            )
                        })?;
                    runtime.block_on(stop.wait());
                    stop_for_fallback.store(true, Ordering::Relaxed);
                    Ok(())
                })
                .map_err(|e| m_error!(EC::ThreadErr, "spawn io_uring stop bridge error", e))?;
            let result = sync_serve_fallback(cfg, stop_flag);
            let notify_result = notifier
                .join()
                .map_err(|_| m_error!(EC::ThreadErr, "join io_uring stop bridge error"))?;
            notify_result?;
            return result;
        }
    }
}

// Non-Linux compatibility path for the historical `IoUringTcpBackend` API.
fn sync_serve_fallback(cfg: IoUringTcpServerConfig, stop: Arc<AtomicBool>) -> RS<()> {
    if cfg.worker_count() == 0 {
        return Err(m_error!(EC::ParseErr, "invalid io_uring worker count"));
    }
    let listen_addr: SocketAddr = format!("{}:{}", cfg.listen_ip(), cfg.listen_port())
        .parse()
        .map_err(|e| m_error!(EC::ParseErr, "parse io_uring tcp listen address error", e))?;

    let conn_id_alloc = Arc::new(AtomicU64::new(1));
    let inboxes: Vec<_> = (0..cfg.worker_count())
        .map(|_| Arc::new(SegQueue::<TransferredConnection>::new()))
        .collect();
    let listener = create_listener(listen_addr)?;

    let mut handles = Vec::with_capacity(cfg.worker_count());
    for worker_id in 0..cfg.worker_count() {
        let worker_count = cfg.worker_count();
        let log_dir = cfg.log_dir().to_string();
        let log_chunk_size = cfg.log_chunk_size();
        let routing_mode = cfg.routing_mode();
        let procedure_runtime = cfg.procedure_runtime_for_worker(worker_id);
        let worker_identity = cfg
            .worker_registry()
            .worker(worker_id)
            .cloned()
            .ok_or_else(|| {
                m_error!(
                    EC::NoSuchElement,
                    format!("missing worker identity {}", worker_id)
                )
            })?;
        let worker_registry = cfg.worker_registry();
        let inbox = inboxes[worker_id].clone();
        let all_inboxes = inboxes.clone();
        let conn_id_alloc = conn_id_alloc.clone();
        let stop = stop.clone();
        let listener = listener
            .try_clone()
            .map_err(|e| m_error!(EC::NetErr, "clone tcp listener error", e))?;
        let handle = thread::Builder::new()
            .name(format!("iouring-tcp-worker-{worker_id}"))
            .spawn(move || {
                let worker = IoUringWorker::new(
                    worker_identity,
                    worker_count,
                    routing_mode,
                    log_dir,
                    log_chunk_size,
                    procedure_runtime,
                    worker_registry,
                )?;
                run_worker_loop(worker, listener, inbox, all_inboxes, conn_id_alloc, stop)
            })
            .map_err(|e| m_error!(EC::ThreadErr, "spawn io_uring worker error", e))?;
        handles.push(handle);
    }

    for handle in handles {
        let result = handle
            .join()
            .map_err(|_| m_error!(EC::ThreadErr, "join io_uring worker error"))?;
        result?;
    }
    Ok(())
}

fn create_listener(listen_addr: SocketAddr) -> RS<TcpListener> {
    let domain = if listen_addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
        .map_err(|e| m_error!(EC::NetErr, "create tcp listener socket error", e))?;
    socket
        .set_reuse_address(true)
        .map_err(|e| m_error!(EC::NetErr, "enable SO_REUSEADDR error", e))?;
    socket
        .set_nonblocking(true)
        .map_err(|e| m_error!(EC::NetErr, "set listener nonblocking error", e))?;
    socket
        .bind(&listen_addr.into())
        .map_err(|e| m_error!(EC::NetErr, "bind io_uring tcp listener error", e))?;
    socket
        .listen(1024)
        .map_err(|e| m_error!(EC::NetErr, "listen io_uring tcp listener error", e))?;
    Ok(socket.into())
}

fn run_worker_loop(
    worker: IoUringWorker,
    listener: TcpListener,
    inbox: Arc<SegQueue<TransferredConnection>>,
    inboxes: Vec<Arc<SegQueue<TransferredConnection>>>,
    conn_id_alloc: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
) -> RS<()> {
    let mut runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| m_error!(EC::TokioErr, "create worker runtime error", e))?;
    let mut connections = HashMap::<u64, WorkerConnection>::new();
    let idle_sleep = Duration::from_millis(1);

    while !stop.load(Ordering::Relaxed) {
        let mut progressed = false;
        progressed |= drain_accepted_connections(
            &listener,
            &worker,
            &inboxes,
            &mut connections,
            &conn_id_alloc,
        )?;
        progressed |= drain_transferred_connections(&worker, inbox.as_ref(), &mut connections)?;
        progressed |= drive_connections(&worker, &mut runtime, &mut connections, &inboxes)?;

        if !progressed {
            thread::sleep(idle_sleep);
        }
    }
    Ok(())
}

fn drain_accepted_connections(
    listener: &TcpListener,
    worker: &IoUringWorker,
    inboxes: &[Arc<SegQueue<TransferredConnection>>],
    connections: &mut HashMap<u64, WorkerConnection>,
    conn_id_alloc: &AtomicU64,
) -> RS<bool> {
    let mut progressed = false;
    loop {
        match listener.accept() {
            Ok((stream, remote_addr)) => {
                progressed = true;
                let conn_id = conn_id_alloc.fetch_add(1, Ordering::Relaxed);
                let target_worker = worker.route_connection(conn_id, remote_addr);
                if target_worker == worker.worker_index() {
                    register_connection(connections, conn_id, remote_addr, stream)?;
                } else {
                    enqueue_transfer(
                        inboxes,
                        conn_id,
                        target_worker,
                        remote_addr,
                        stream,
                        Vec::new(),
                        None,
                    )?;
                }
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => break,
            Err(err) => {
                return Err(m_error!(
                    EC::NetErr,
                    "accept io_uring tcp connection error",
                    err
                ));
            }
        }
    }
    Ok(progressed)
}

fn enqueue_transfer(
    inboxes: &[Arc<SegQueue<TransferredConnection>>],
    conn_id: u64,
    target_worker: usize,
    remote_addr: SocketAddr,
    stream: TcpStream,
    session_ids: Vec<OID>,
    session_open_action: Option<SessionOpenTransferAction>,
) -> RS<()> {
    let target_inbox = inboxes.get(target_worker).ok_or_else(|| {
        m_error!(
            EC::InternalErr,
            format!("route target worker {} is out of range", target_worker)
        )
    })?;
    target_inbox.push(TransferredConnection {
        transfer: ConnectionTransfer::new(
            conn_id,
            target_worker,
            crate::server_ur::fsm::ConnectionState::Accepted,
            remote_addr,
        ),
        stream,
        session_ids,
        session_open_action,
    });
    Ok(())
}

fn drain_transferred_connections(
    worker: &IoUringWorker,
    inbox: &SegQueue<TransferredConnection>,
    connections: &mut HashMap<u64, WorkerConnection>,
) -> RS<bool> {
    let mut progressed = false;
    while let Some(connection) = inbox.pop() {
        progressed = true;
        worker.adopt_connection_sessions(connection.transfer.conn_id(), &connection.session_ids)?;
        register_connection(
            connections,
            connection.transfer.conn_id(),
            connection.transfer.remote_addr(),
            connection.stream,
        )?;
        if let Some(action) = connection.session_open_action {
            let payload = match worker
                .open_session_with_config(connection.transfer.conn_id(), action.config())
            {
                Ok(session_id) => encode_session_create_response(
                    action.request_id(),
                    &SessionCreateResponse::new(session_id),
                )?,
                Err(err) => encode_error_response(action.request_id(), err.to_string())?,
            };
            if let Some(registered) = connections.get_mut(&connection.transfer.conn_id()) {
                registered.write_buf.extend_from_slice(&payload);
            }
        }
    }
    Ok(progressed)
}

fn register_connection(
    connections: &mut HashMap<u64, WorkerConnection>,
    conn_id: u64,
    remote_addr: SocketAddr,
    stream: TcpStream,
) -> RS<()> {
    stream
        .set_nonblocking(true)
        .map_err(|e| m_error!(EC::NetErr, "set connection nonblocking error", e))?;
    stream
        .set_nodelay(true)
        .map_err(|e| m_error!(EC::NetErr, "set connection nodelay error", e))?;
    connections.insert(
        conn_id,
        WorkerConnection {
            conn_id,
            state: crate::server_ur::fsm::ConnectionState::Active,
            stream,
            remote_addr,
            transferred: false,
            read_buf: Vec::with_capacity(4096),
            write_buf: Vec::with_capacity(4096),
        },
    );
    Ok(())
}

fn drive_connections(
    worker: &IoUringWorker,
    runtime: &mut tokio::runtime::Runtime,
    connections: &mut HashMap<u64, WorkerConnection>,
    inboxes: &[Arc<SegQueue<TransferredConnection>>],
) -> RS<bool> {
    let mut progressed = false;
    let conn_ids: Vec<u64> = connections.keys().copied().collect();
    let mut closed = Vec::new();

    for conn_id in conn_ids {
        let Some(connection) = connections.get_mut(&conn_id) else {
            continue;
        };
        progressed |= flush_pending_writes(connection)?;
        let connection_progress = read_and_dispatch(worker, runtime, connection, inboxes)?;
        progressed |= connection_progress;
        if connection.state == crate::server_ur::fsm::ConnectionState::Closing
            && connection.write_buf.is_empty()
        {
            closed.push((conn_id, connection.transferred));
        }
    }

    for (conn_id, transferred) in closed {
        if !transferred {
            worker.close_connection_sessions(conn_id)?;
        }
        connections.remove(&conn_id);
    }
    Ok(progressed)
}

fn flush_pending_writes(connection: &mut WorkerConnection) -> RS<bool> {
    let mut progressed = false;
    while !connection.write_buf.is_empty() {
        match connection.stream.write(&connection.write_buf) {
            Ok(0) => {
                connection.state = crate::server_ur::fsm::ConnectionState::Closing;
                break;
            }
            Ok(written) => {
                progressed = true;
                connection.write_buf.drain(0..written);
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => break,
            Err(err) => return Err(m_error!(EC::NetErr, "write tcp response error", err)),
        }
    }
    Ok(progressed)
}

fn read_and_dispatch(
    worker: &IoUringWorker,
    runtime: &mut tokio::runtime::Runtime,
    connection: &mut WorkerConnection,
    inboxes: &[Arc<SegQueue<TransferredConnection>>],
) -> RS<bool> {
    let mut progressed = false;
    let mut buf = [0u8; 8192];
    loop {
        match connection.stream.read(&mut buf) {
            Ok(0) => {
                connection.state = crate::server_ur::fsm::ConnectionState::Closing;
                break;
            }
            Ok(read) => {
                progressed = true;
                connection.read_buf.extend_from_slice(&buf[..read]);
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => break,
            Err(err) => return Err(m_error!(EC::NetErr, "read tcp request error", err)),
        }
    }

    while let Some((frame, consumed)) = try_decode_next_frame(&connection.read_buf)? {
        progressed = true;
        let response = dispatch_frame(worker, connection.conn_id, runtime, &frame);
        connection.read_buf.drain(0..consumed);
        match response {
            Ok(DispatchFrameResult::Immediate(payload)) => {
                connection.write_buf.extend_from_slice(&payload);
            }
            Ok(DispatchFrameResult::Transfer {
                target_worker,
                session_ids,
                action,
            }) => {
                let stream = connection
                    .stream
                    .try_clone()
                    .map_err(|e| m_error!(EC::NetErr, "clone transferred stream error", e))?;
                enqueue_transfer(
                    inboxes,
                    connection.conn_id,
                    target_worker,
                    connection.remote_addr,
                    stream,
                    session_ids,
                    Some(action),
                )?;
                connection.transferred = true;
                connection.state = crate::server_ur::fsm::ConnectionState::Closing;
                connection.write_buf.clear();
                return Ok(true);
            }
            Err(err) => {
                let payload = encode_error_response(frame.header().request_id(), err.to_string())?;
                connection.write_buf.extend_from_slice(&payload);
            }
        }
    }
    Ok(progressed)
}

fn try_decode_next_frame(buf: &[u8]) -> RS<Option<(Frame, usize)>> {
    if buf.len() < HEADER_LEN {
        return Ok(None);
    }
    let payload_len = u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]) as usize;
    let frame_len = HEADER_LEN + payload_len;
    if buf.len() < frame_len {
        return Ok(None);
    }
    let frame = Frame::decode(&buf[..frame_len])?;
    Ok(Some((frame, frame_len)))
}

fn dispatch_frame(
    worker: &IoUringWorker,
    conn_id: u64,
    runtime: &mut tokio::runtime::Runtime,
    frame: &Frame,
) -> RS<DispatchFrameResult> {
    match frame.header().message_type() {
        MessageType::Query | MessageType::Execute => {
            let request = decode_client_request(frame)?;
            Ok(DispatchFrameResult::Immediate(encode_server_response(
                frame.header().request_id(),
                &ServerResponse::new(
                    vec![],
                    vec![],
                    0,
                    Some(format!(
                        "SQL interface is disabled in the client backend for app '{}'",
                        request.app_name()
                    )),
                ),
            )?))
        }
        MessageType::Get => {
            let request = decode_get_request(frame)?;
            let value = worker.get_for_connection(conn_id, request.session_id(), request.key())?;
            Ok(DispatchFrameResult::Immediate(encode_get_response(
                frame.header().request_id(),
                &GetResponse::new(value),
            )?))
        }
        MessageType::Put => {
            let request = decode_put_request(frame)?;
            let session_id = request.session_id();
            let (key, value) = request.into_parts();
            worker.put_for_connection(conn_id, session_id, key, value)?;
            Ok(DispatchFrameResult::Immediate(encode_put_response(
                frame.header().request_id(),
                &PutResponse::new(true),
            )?))
        }
        MessageType::RangeScan => {
            let request = decode_range_scan_request(frame)?;
            let items = worker.range_for_connection(
                conn_id,
                request.session_id(),
                request.start_key(),
                request.end_key(),
            )?;
            Ok(DispatchFrameResult::Immediate(encode_range_scan_response(
                frame.header().request_id(),
                &RangeScanResponse::new(
                    items
                        .into_iter()
                        .map(|item| KeyValue::new(item.key, item.value))
                        .collect(),
                ),
            )?))
        }
        MessageType::ProcedureInvoke => {
            let request = decode_procedure_invoke_request(frame)?;
            let response = runtime.block_on(worker.handle_procedure_request(conn_id, &request))?;
            Ok(DispatchFrameResult::Immediate(
                encode_procedure_invoke_response(frame.header().request_id(), &response)?,
            ))
        }
        MessageType::SessionCreate => {
            let request = decode_session_create_request(frame)?;
            let config = parse_session_open_config(
                request.config_json(),
                worker.worker_index(),
                worker.worker_id(),
                worker.registry().as_ref(),
            )?;
            if config.target_worker_index() == worker.worker_index() {
                Ok(DispatchFrameResult::Immediate(
                    encode_session_create_response(
                        frame.header().request_id(),
                        &SessionCreateResponse::new(
                            worker.open_session_with_config(conn_id, config)?,
                        ),
                    )?,
                ))
            } else {
                let action = SessionOpenTransferAction::new(frame.header().request_id(), config);
                let session_ids = worker.prepare_connection_transfer(conn_id, Some(action))?;
                Ok(DispatchFrameResult::Transfer {
                    target_worker: config.target_worker_index(),
                    session_ids,
                    action,
                })
            }
        }
        MessageType::SessionClose => {
            let request = decode_session_close_request(frame)?;
            Ok(DispatchFrameResult::Immediate(
                encode_session_close_response(
                    frame.header().request_id(),
                    &SessionCloseResponse::new(
                        worker.close_session(conn_id, request.session_id())?,
                    ),
                )?,
            ))
        }
        MessageType::Handshake | MessageType::Auth | MessageType::Response | MessageType::Error => {
            Err(m_error!(
                EC::ParseErr,
                format!(
                    "unsupported client message type {:?}",
                    frame.header().message_type()
                )
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mudu_contract::protocol::encode_get_request;
    use mudu_contract::protocol::GetRequest;

    #[test]
    fn try_decode_next_frame_waits_for_full_payload() {
        let encoded = encode_get_request(1, &GetRequest::new(1, b"k".to_vec())).unwrap();
        assert!(try_decode_next_frame(&encoded[..HEADER_LEN - 1])
            .unwrap()
            .is_none());
        assert!(try_decode_next_frame(&encoded[..HEADER_LEN])
            .unwrap()
            .is_none());
        let decoded = try_decode_next_frame(&encoded).unwrap().unwrap();
        assert_eq!(decoded.0.header().request_id(), 1);
        assert_eq!(decoded.1, encoded.len());
    }
}
