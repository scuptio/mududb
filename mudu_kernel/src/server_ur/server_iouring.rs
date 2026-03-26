use crate::server_ur::pending_procedure_invocation::PendingProcedureInvocation;
use crate::server_ur::procedure_task::{FrameDispatch, SessionTransferDispatch};
use crate::server_ur::routing::{SessionOpenTransferAction, parse_session_open_config};
use crate::server_ur::server::IoUringTcpServerConfig;
use crate::server_ur::worker::IoUringWorker;
use crate::server_ur::worker_local_log::WorkerLocalLog;
use crate::server_ur::worker_mailbox::WorkerMailboxMsg;
use crate::server_ur::worker_ring_loop::{WorkerLoopStats, WorkerRingLoop};
use crossbeam_queue::SegQueue;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::protocol::{
    Frame, GetResponse, HEADER_LEN, KeyValue, MessageType, PutResponse, RangeScanResponse,
    ServerResponse, SessionCloseResponse, SessionCreateResponse, decode_client_request,
    decode_get_request, decode_procedure_invoke_request, decode_put_request,
    decode_range_scan_request, decode_session_close_request, decode_session_create_request,
    encode_get_response, encode_put_response, encode_range_scan_response, encode_server_response,
    encode_session_close_response, encode_session_create_response,
};
use mudu_utils::notifier::Waiter;
use socket2::{Domain, Protocol, Socket, Type};
use std::os::fd::{AsRawFd, IntoRawFd, RawFd};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use tracing::{debug, info};

pub(crate) struct RecoveryCoordinator {
    total_workers: usize,
    state: Mutex<RecoveryState>,
    condvar: Condvar,
}

#[derive(Default)]
struct RecoveryState {
    recovered_workers: usize,
    failed: bool,
}

pub(crate) fn sync_serve_iouring(cfg: IoUringTcpServerConfig, stop: Waiter) -> RS<()> {
    if cfg.worker_count() == 0 {
        return Err(m_error!(EC::ParseErr, "invalid io_uring worker count"));
    }
    let listen_addr: std::net::SocketAddr = format!("{}:{}", cfg.listen_ip(), cfg.listen_port())
        .parse()
        .map_err(|e| m_error!(EC::ParseErr, "parse io_uring tcp listen address error", e))?;
    let conn_id_alloc = Arc::new(AtomicU64::new(1));
    let mailboxes: Vec<_> = (0..cfg.worker_count())
        .map(|_| Arc::new(SegQueue::<WorkerMailboxMsg>::new()))
        .collect();
    let mailbox_fds: Vec<_> = (0..cfg.worker_count())
        .map(|_| create_mailbox_event_fd())
        .collect::<RS<Vec<_>>>()?;
    let stop_flag = Arc::new(AtomicBool::new(false));
    let recovery_coordinator = Arc::new(RecoveryCoordinator::new(cfg.worker_count()));

    let stop_for_notifier = stop.clone();
    let shutdown_mailboxes = mailboxes.clone();
    let shutdown_mailbox_fds = mailbox_fds.clone();
    let notifier_stop_flag = stop_flag.clone();
    let notifier = thread::Builder::new()
        .name("iouring-shutdown-notifier".to_string())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| {
                    m_error!(
                        EC::TokioErr,
                        "create runtime for io_uring shutdown notifier error",
                        e
                    )
                })?;
            runtime.block_on(stop_for_notifier.wait());
            notifier_stop_flag.store(true, Ordering::Relaxed);
            for (mailbox, fd) in shutdown_mailboxes
                .into_iter()
                .zip(shutdown_mailbox_fds.into_iter())
            {
                mailbox.push(WorkerMailboxMsg::Shutdown);
                notify_mailbox_fd(fd)?;
            }
            debug!("notify shutdown");
            Ok(())
        })
        .map_err(|e| m_error!(EC::ThreadErr, "spawn io_uring shutdown notifier error", e))?;

    let mut handles = Vec::with_capacity(cfg.worker_count());
    for worker_id in 0..cfg.worker_count() {
        let listen_addr = listen_addr;
        let conn_id_alloc = conn_id_alloc.clone();
        let mailbox = mailboxes[worker_id].clone();
        let all_mailboxes = mailboxes.clone();
        let all_mailbox_fds = mailbox_fds.clone();
        let procedure_runtime = cfg.procedure_runtime_for_worker(worker_id);
        let routing_mode = cfg.routing_mode();
        let log_dir = cfg.log_dir().to_string();
        let log_chunk_size = cfg.log_chunk_size();
        let worker_count = cfg.worker_count();
        let stop = stop_flag.clone();
        let recovery_coordinator = recovery_coordinator.clone();
        let mailbox_fd = mailbox_fds[worker_id];
        let handle = thread::Builder::new()
            .name(format!("iouring-ring-worker-{worker_id}"))
            .spawn(move || {
                let listener_fd = create_listener_fd(listen_addr)?;
                let worker = IoUringWorker::new(
                    worker_id,
                    worker_count,
                    routing_mode,
                    log_dir.clone(),
                    log_chunk_size,
                    procedure_runtime,
                )?;
                let log = WorkerLocalLog::open(worker.log_layout())?;
                let mut loop_state = WorkerRingLoop::new(
                    worker,
                    listener_fd,
                    mailbox_fd,
                    mailbox,
                    all_mailboxes,
                    all_mailbox_fds,
                    conn_id_alloc,
                    log,
                    recovery_coordinator,
                    stop,
                )?;
                let r = loop_state.run();
                r
            })
            .map_err(|e| m_error!(EC::ThreadErr, "spawn io_uring worker error", e))?;
        handles.push(handle);
    }

    let mut worker_stats = Vec::<WorkerLoopStats>::with_capacity(cfg.worker_count());
    for handle in handles {
        let result = handle
            .join()
            .map_err(|_| m_error!(EC::ThreadErr, "join io_uring worker error"))?;
        worker_stats.push(result?);
    }

    let notify_result = notifier
        .join()
        .map_err(|_| m_error!(EC::ThreadErr, "join io_uring shutdown notifier error"))?;
    notify_result?;

    for fd in mailbox_fds {
        unsafe {
            libc::close(fd);
        }
    }
    log_worker_stats(&worker_stats);
    Ok(())
}

impl RecoveryCoordinator {
    pub(crate) fn new(total_workers: usize) -> Self {
        Self {
            total_workers,
            state: Mutex::new(RecoveryState::default()),
            condvar: Condvar::new(),
        }
    }

    pub(crate) fn worker_succeeded(&self) -> RS<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "recovery coordinator lock poisoned"))?;
        if state.failed {
            return Err(m_error!(
                EC::ThreadErr,
                "worker recovery aborted because another worker failed"
            ));
        }
        state.recovered_workers += 1;
        if state.recovered_workers == self.total_workers {
            self.condvar.notify_all();
            return Ok(());
        }
        // Recovery must be complete on every worker before the service loop
        // starts. If one worker fails recovery, wake everybody and abort
        // instead of leaving the successful workers stuck forever.
        while !state.failed && state.recovered_workers < self.total_workers {
            state = self.condvar.wait(state).map_err(|_| {
                m_error!(
                    EC::InternalErr,
                    "recovery coordinator condvar wait poisoned"
                )
            })?;
        }
        if state.failed {
            return Err(m_error!(
                EC::ThreadErr,
                "worker recovery aborted because another worker failed"
            ));
        }
        Ok(())
    }

    pub(crate) fn worker_failed(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.failed = true;
            self.condvar.notify_all();
        }
    }
}

pub fn dispatch_frame_iouring(
    worker: &IoUringWorker,
    conn_id: u64,
    frame: &Frame,
) -> RS<FrameDispatch> {
    match frame.header().message_type() {
        MessageType::Query | MessageType::Execute => {
            let request = decode_client_request(frame)?;
            Ok(FrameDispatch::Immediate(encode_server_response(
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
            Ok(FrameDispatch::Immediate(encode_get_response(
                frame.header().request_id(),
                &GetResponse::new(value),
            )?))
        }
        MessageType::Put => {
            let request = decode_put_request(frame)?;
            let session_id = request.session_id();
            let (key, value) = request.into_parts();
            worker.put_for_connection(conn_id, session_id, key, value)?;
            Ok(FrameDispatch::Immediate(encode_put_response(
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
            Ok(FrameDispatch::Immediate(encode_range_scan_response(
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
            let worker = worker.clone();
            let request_id = frame.header().request_id();
            let completed = Arc::new(AtomicBool::new(false));
            Ok(FrameDispatch::Pending(PendingProcedureInvocation::new(
                conn_id,
                request_id,
                // The component invoke future is polled from the worker loop so
                // the ring thread never blocks on procedure execution.
                completed.clone(),
                Box::pin(async move {
                    let response = worker.handle_procedure_request(conn_id, &request).await;
                    completed.store(true, Ordering::SeqCst);
                    Ok(response?.into_result())
                }),
            )))
        }
        MessageType::SessionCreate => {
            let request = decode_session_create_request(frame)?;
            let config = parse_session_open_config(
                request.config_json(),
                worker.partition_id(),
                worker.worker_count(),
            )?;
            if config.partition_id() == worker.partition_id() {
                let session_id = worker.open_session_with_config(conn_id, config)?;
                Ok(FrameDispatch::Immediate(encode_session_create_response(
                    frame.header().request_id(),
                    &SessionCreateResponse::new(session_id),
                )?))
            } else {
                let action = SessionOpenTransferAction::new(frame.header().request_id(), config);
                let session_ids = worker.prepare_connection_transfer(conn_id, Some(action))?;
                Ok(FrameDispatch::Transfer(SessionTransferDispatch::new(
                    config.partition_id(),
                    session_ids,
                    action,
                )))
            }
        }
        MessageType::SessionClose => {
            let request = decode_session_close_request(frame)?;
            let closed = worker.close_session(conn_id, request.session_id())?;
            Ok(FrameDispatch::Immediate(encode_session_close_response(
                frame.header().request_id(),
                &SessionCloseResponse::new(closed),
            )?))
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

pub fn try_decode_next_frame(buf: &[u8]) -> RS<Option<(Frame, usize)>> {
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

fn create_listener_fd(listen_addr: std::net::SocketAddr) -> RS<RawFd> {
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
    enable_reuse_port(&socket)?;
    socket
        .bind(&listen_addr.into())
        .map_err(|e| m_error!(EC::NetErr, "bind io_uring tcp listener error", e))?;
    socket
        .listen(1024)
        .map_err(|e| m_error!(EC::NetErr, "listen io_uring tcp listener error", e))?;
    Ok(socket.into_raw_fd())
}

fn enable_reuse_port(socket: &Socket) -> RS<()> {
    let value: libc::c_int = 1;
    let rc = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            &value as *const _ as *const libc::c_void,
            std::mem::size_of_val(&value) as libc::socklen_t,
        )
    };
    if rc != 0 {
        return Err(m_error!(
            EC::NetErr,
            "enable SO_REUSEPORT error",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

pub fn set_connection_options(fd: RawFd) -> RS<()> {
    let flag: libc::c_int = 1;
    let rc = unsafe {
        libc::setsockopt(
            fd,
            libc::IPPROTO_TCP,
            libc::TCP_NODELAY,
            &flag as *const _ as *const libc::c_void,
            std::mem::size_of_val(&flag) as libc::socklen_t,
        )
    };
    if rc != 0 {
        return Err(m_error!(
            EC::NetErr,
            "set connection nodelay error",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn create_mailbox_event_fd() -> RS<RawFd> {
    create_event_fd("create io_uring worker mailbox eventfd error")
}

fn create_event_fd(message: &str) -> RS<RawFd> {
    let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };
    if fd < 0 {
        return Err(m_error!(
            EC::NetErr,
            message,
            std::io::Error::last_os_error()
        ));
    }
    Ok(fd)
}

pub(super) fn notify_mailbox_fd(fd: RawFd) -> RS<()> {
    notify_event_fd(fd, "write io_uring worker mailbox eventfd error")
}

fn notify_event_fd(fd: RawFd, message: &str) -> RS<()> {
    let value: u64 = 1;
    let rc = unsafe {
        libc::write(
            fd,
            &value as *const u64 as *const libc::c_void,
            std::mem::size_of::<u64>(),
        )
    };
    if rc as usize != std::mem::size_of::<u64>() {
        return Err(m_error!(
            EC::NetErr,
            message,
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn log_worker_stats(stats: &[WorkerLoopStats]) {
    for stat in stats {
        debug!(
            "iouring worker stats: \n\
            worker={}, submit_calls={}, wait_cqe_calls={}, \n\
            accept_submit={}, mailbox_submit={}, recv_submit={}, send_submit={}, \
            log_write_submit={}, cqe_accept={}, cqe_mailbox={}, cqe_recv={}, cqe_send={}, \
            cqe_log_write={}, cqe_close={}, recv_queue_push={}, recv_queue_pop={}, \
            send_queue_push={}, send_queue_pop={}, mailbox_drained={}, local_register={}",
            stat.worker_id,
            stat.submit_calls,
            stat.wait_cqe_calls,
            stat.accept_submit,
            stat.mailbox_submit,
            stat.recv_submit,
            stat.send_submit,
            stat.log_write_submit,
            stat.cqe_accept,
            stat.cqe_mailbox,
            stat.cqe_recv,
            stat.cqe_send,
            stat.cqe_log_write,
            stat.cqe_close,
            stat.recv_queue_push,
            stat.recv_queue_pop,
            stat.send_queue_push,
            stat.send_queue_pop,
            stat.mailbox_drained,
            stat.local_register,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_ur::routing::ConnectionTransfer;
    use crate::server_ur::transferred_connection::TransferredConnection;

    #[test]
    fn mailbox_eventfd_accumulates_wakeups() {
        let fd = create_mailbox_event_fd().unwrap();
        notify_mailbox_fd(fd).unwrap();
        notify_mailbox_fd(fd).unwrap();

        let mut value = 0u64;
        let rc = unsafe {
            libc::read(
                fd,
                (&mut value) as *mut u64 as *mut libc::c_void,
                std::mem::size_of::<u64>(),
            )
        };
        assert_eq!(rc as usize, std::mem::size_of::<u64>());
        assert_eq!(value, 2);

        unsafe {
            libc::close(fd);
        }
    }

    #[test]
    fn mailbox_can_store_shutdown_and_transfer_messages() {
        let mailbox = SegQueue::new();
        mailbox.push(WorkerMailboxMsg::AdoptConnection(
            TransferredConnection::new(
                ConnectionTransfer::new(
                    11,
                    1,
                    crate::server_ur::fsm::ConnectionState::Accepted,
                    "127.0.0.1:9527".parse().unwrap(),
                ),
                -1,
                Vec::new(),
                None,
            ),
        ));
        mailbox.push(WorkerMailboxMsg::Shutdown);
        match mailbox.pop() {
            Some(WorkerMailboxMsg::AdoptConnection(connection)) => {
                assert_eq!(connection.transfer().conn_id(), 11);
                assert_eq!(connection.transfer().target_worker(), 1);
            }
            other => panic!("unexpected first mailbox message: {other:?}"),
        }
        assert!(matches!(mailbox.pop(), Some(WorkerMailboxMsg::Shutdown)));
        assert!(mailbox.pop().is_none());
    }
}

pub fn sockaddr_to_socket_addr(
    storage: &rliburing::sockaddr_storage,
    _addr_len: rliburing::socklen_t,
) -> RS<std::net::SocketAddr> {
    match storage.ss_family as i32 {
        libc::AF_INET => {
            let addr: libc::sockaddr_in =
                unsafe { std::ptr::read(storage as *const _ as *const _) };
            let ip = std::net::Ipv4Addr::from(u32::from_be(addr.sin_addr.s_addr).to_be_bytes());
            let port = u16::from_be(addr.sin_port);
            Ok(std::net::SocketAddr::from((ip, port)))
        }
        libc::AF_INET6 => {
            let addr: libc::sockaddr_in6 =
                unsafe { std::ptr::read(storage as *const _ as *const _) };
            let ip = std::net::Ipv6Addr::from(addr.sin6_addr.s6_addr);
            let port = u16::from_be(addr.sin6_port);
            Ok(std::net::SocketAddr::from((ip, port)))
        }
        family => Err(m_error!(
            EC::NetErr,
            format!("unsupported socket family {}", family)
        )),
    }
}
