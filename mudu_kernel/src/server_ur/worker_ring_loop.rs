use crate::server_ur::inflight_op::{AcceptOp, CloseFileOp, InflightOp, OpenFileOp};
use crate::server_ur::procedure_task::{FrameDispatch, ProcedureTask};
use crate::server_ur::procedure_task_waker::ProcedureTaskWaker;
use crate::server_ur::server_iouring;
use crate::server_ur::server_iouring::RecoveryCoordinator;
use crate::server_ur::worker::IoUringWorker;
use crate::server_ur::worker_connection::WorkerConnection;
use crate::server_ur::worker_local_log::{InflightLogWrite, WorkerLocalLog};
use crate::server_ur::worker_mailbox::WorkerMailboxMsg;
use crate::x_log::worker_kv_log::WorkerKvLog;
use crossbeam_queue::SegQueue;
use futures::task::waker;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::protocol::{
    encode_error_response, encode_procedure_invoke_response, ProcedureInvokeResponse,
};
use mudu_contract::protocol::{encode_session_create_response, SessionCreateResponse};
use std::collections::{HashMap, VecDeque};
use std::fs::OpenOptions;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;

#[derive(Debug, Default, Clone)]
pub(in crate::server_ur) struct WorkerLoopStats {
    pub worker_id: usize,
    pub submit_calls: u64,
    pub wait_cqe_calls: u64,
    pub cqe_accept: u64,
    pub cqe_mailbox: u64,
    pub cqe_recv: u64,
    pub cqe_send: u64,
    pub cqe_log_open: u64,
    pub cqe_file_close: u64,
    pub cqe_log_write: u64,
    pub cqe_close: u64,
    pub recv_queue_push: u64,
    pub recv_queue_pop: u64,
    pub send_queue_push: u64,
    pub send_queue_pop: u64,
    pub recv_submit: u64,
    pub send_submit: u64,
    pub log_open_submit: u64,
    pub file_close_submit: u64,
    pub log_write_submit: u64,
    pub accept_submit: u64,
    pub mailbox_submit: u64,
    pub mailbox_drained: u64,
    pub local_register: u64,
}

pub(in crate::server_ur) struct WorkerRingLoop {
    worker: IoUringWorker,
    ring: rliburing::io_uring,
    listener_fd: RawFd,
    mailbox_fd: RawFd,
    mailbox: Arc<SegQueue<WorkerMailboxMsg>>,
    mailboxes: Vec<Arc<SegQueue<WorkerMailboxMsg>>>,
    mailbox_fds: Vec<RawFd>,
    conn_id_alloc: Arc<AtomicU64>,
    log: WorkerLocalLog,
    recovery_coordinator: Arc<RecoveryCoordinator>,
    connections: HashMap<u64, Box<WorkerConnection>>,
    ready_recv: VecDeque<u64>,
    ready_send: VecDeque<u64>,
    inflight: HashMap<u64, InflightOp>,
    procedure_tasks: HashMap<u64, ProcedureTask>,
    // Ready tasks are polled immediately by the worker loop.
    procedure_ready_queue: Arc<SegQueue<u64>>,
    // Completion notifications are translated back into task ids before the
    // next poll, which keeps the scheduling model close to io_uring CQEs.
    procedure_completion_queue: Arc<SegQueue<u64>>,
    // Maps a worker-local async op id to the suspended procedure task waiting
    // for that completion.
    procedure_op_registry: HashMap<u64, u64>,
    next_token: u64,
    next_task_id: u64,
    next_op_id: u64,
    mailbox_read_submitted: bool,
    shutdown_triggered: bool,
    shutting_down: bool,
    accept_submitted: bool,
    stop: Arc<AtomicBool>,
    stats: WorkerLoopStats,
}

impl WorkerRingLoop {
    pub(in crate::server_ur) fn new(
        worker: IoUringWorker,
        listener_fd: RawFd,
        mailbox_fd: RawFd,
        mailbox: Arc<SegQueue<WorkerMailboxMsg>>,
        mailboxes: Vec<Arc<SegQueue<WorkerMailboxMsg>>>,
        mailbox_fds: Vec<RawFd>,
        conn_id_alloc: Arc<AtomicU64>,
        log: WorkerLocalLog,
        recovery_coordinator: Arc<RecoveryCoordinator>,
        stop: Arc<AtomicBool>,
    ) -> RS<Self> {
        let mut ring: rliburing::io_uring = unsafe { std::mem::zeroed() };
        let mut param: rliburing::io_uring_params = unsafe { std::mem::zeroed() };
        let worker_id = worker.worker_index();
        let rc = unsafe { rliburing::io_uring_queue_init_params(1024, &mut ring, &mut param) };
        if rc != 0 {
            return Err(m_error!(
                EC::NetErr,
                format!("io_uring_queue_init_params error {}", rc)
            ));
        }
        Ok(Self {
            worker,
            ring,
            listener_fd,
            mailbox_fd,
            mailbox,
            mailboxes,
            mailbox_fds,
            conn_id_alloc,
            log,
            recovery_coordinator,
            connections: HashMap::new(),
            ready_recv: VecDeque::new(),
            ready_send: VecDeque::new(),
            inflight: HashMap::new(),
            procedure_tasks: HashMap::new(),
            procedure_ready_queue: Arc::new(SegQueue::new()),
            procedure_completion_queue: Arc::new(SegQueue::new()),
            procedure_op_registry: HashMap::new(),
            next_token: 1,
            next_task_id: 1,
            next_op_id: 1,
            mailbox_read_submitted: false,
            shutdown_triggered: false,
            shutting_down: false,
            accept_submitted: false,
            stop,
            stats: WorkerLoopStats {
                worker_id,
                ..WorkerLoopStats::default()
            },
        })
    }

    pub(in crate::server_ur) fn run(&mut self) -> RS<WorkerLoopStats> {
        if let Err(err) = self.recover_worker_log() {
            self.recovery_coordinator.worker_failed();
            return Err(err);
        }
        self.recovery_coordinator.worker_succeeded()?;
        self.run_service_loop()
    }

    fn run_service_loop(&mut self) -> RS<WorkerLoopStats> {
        loop {
            if self.stop.load(Ordering::Relaxed) || self.shutdown_triggered {
                self.begin_shutdown()?;
            }
            if !self.shutting_down {
                self.drain_mailbox()?;
                self.drain_procedure_completions();
                self.poll_ready_procedures()?;
            } else {
                self.submit_close_for_drained_connections()?;
            }
            self.submit_mailbox_read_if_needed()?;
            self.submit_accept_if_needed()?;
            self.submit_recv_ops()?;
            self.submit_send_ops()?;
            self.submit_log_open_if_needed()?;
            self.submit_file_close_if_needed()?;
            self.submit_log_write_if_needed()?;
            self.stats.submit_calls += 1;
            let submitted = unsafe { rliburing::io_uring_submit(&mut self.ring) };
            if submitted < 0 {
                return Err(m_error!(
                    EC::NetErr,
                    format!("io_uring_submit error {}", submitted)
                ));
            }

            if self.shutting_down && self.connections.is_empty() {
                return self.finish_shutdown();
            }

            if self.inflight.is_empty() {
                thread::sleep(Duration::from_millis(1));
                continue;
            }

            let mut cqe_ptr: *mut rliburing::io_uring_cqe = std::ptr::null_mut();
            self.stats.wait_cqe_calls += 1;
            let wait_rc = unsafe { rliburing::io_uring_wait_cqe(&mut self.ring, &mut cqe_ptr) };
            if wait_rc == -libc::EINTR {
                // A signal interrupted the wait. The ring state is still valid,
                // so the worker should simply retry instead of aborting.
                continue;
            }
            if wait_rc < 0 {
                return Err(m_error!(
                    EC::NetErr,
                    format!("io_uring_wait_cqe error {}", wait_rc)
                ));
            }
            self.process_cqe(cqe_ptr)?;

            loop {
                let mut next_cqe_ptr: *mut rliburing::io_uring_cqe = std::ptr::null_mut();
                let peek_rc =
                    unsafe { rliburing::io_uring_peek_cqe(&mut self.ring, &mut next_cqe_ptr) };
                if peek_rc == -libc::EAGAIN || next_cqe_ptr.is_null() {
                    break;
                }
                if peek_rc < 0 {
                    return Err(m_error!(
                        EC::NetErr,
                        format!("io_uring_peek_cqe error {}", peek_rc)
                    ));
                }
                self.process_cqe(next_cqe_ptr)?;
            }
        }
    }

    fn recover_worker_log(&mut self) -> RS<()> {
        let chunk_paths = self.worker.log_layout().chunk_paths_sorted()?;
        for path in chunk_paths {
            let file = OpenOptions::new()
                .read(true)
                .open(&path)
                .map_err(|e| m_error!(EC::IOErr, "open worker log chunk for recovery error", e))?;
            let size = file
                .metadata()
                .map_err(|e| {
                    m_error!(
                        EC::IOErr,
                        "read worker log chunk recovery metadata error",
                        e
                    )
                })?
                .len() as usize;
            if size == 0 {
                continue;
            }
            let bytes = self.read_file_all_iouring(&file, size)?;
            for (key, value) in WorkerKvLog::decode_put_records(&bytes)? {
                self.worker.replay_log_entry(key, value)?;
            }
        }
        Ok(())
    }

    fn read_file_all_iouring(&mut self, file: &std::fs::File, size: usize) -> RS<Vec<u8>> {
        let mut buf = vec![0u8; size];
        let mut offset = 0usize;
        while offset < size {
            let sqe = unsafe { rliburing::io_uring_get_sqe(&mut self.ring) };
            if sqe.is_null() {
                let submitted = unsafe { rliburing::io_uring_submit(&mut self.ring) };
                if submitted < 0 {
                    return Err(m_error!(
                        EC::IOErr,
                        format!("submit io_uring recovery read error {}", submitted)
                    ));
                }
                continue;
            }
            unsafe {
                (*sqe).user_data = 0;
                rliburing::io_uring_prep_read(
                    sqe,
                    file.as_raw_fd(),
                    buf[offset..].as_mut_ptr() as *mut libc::c_void,
                    (size - offset) as _,
                    offset as _,
                );
            }
            let submitted = unsafe { rliburing::io_uring_submit(&mut self.ring) };
            if submitted < 0 {
                return Err(m_error!(
                    EC::IOErr,
                    format!("submit io_uring recovery read error {}", submitted)
                ));
            }
            let mut cqe_ptr: *mut rliburing::io_uring_cqe = std::ptr::null_mut();
            let wait_rc = unsafe { rliburing::io_uring_wait_cqe(&mut self.ring, &mut cqe_ptr) };
            if wait_rc < 0 {
                return Err(m_error!(
                    EC::IOErr,
                    format!("wait io_uring recovery read cqe error {}", wait_rc)
                ));
            }
            let read = unsafe { (*cqe_ptr).res };
            unsafe { rliburing::io_uring_cqe_seen(&mut self.ring, cqe_ptr) };
            if read < 0 {
                return Err(m_error!(
                    EC::IOErr,
                    format!("worker log recovery read completion error {}", read)
                ));
            }
            if read == 0 {
                break;
            }
            offset += read as usize;
        }
        buf.truncate(offset);
        Ok(buf)
    }

    pub(in crate::server_ur) fn process_cqe(
        &mut self,
        cqe_ptr: *mut rliburing::io_uring_cqe,
    ) -> RS<()> {
        let token = unsafe { (*cqe_ptr).user_data };
        let result = unsafe { (*cqe_ptr).res };
        unsafe { rliburing::io_uring_cqe_seen(&mut self.ring, cqe_ptr) };
        let op = self.inflight.remove(&token).ok_or_else(|| {
            m_error!(
                EC::InternalErr,
                format!("unknown io_uring completion token {}", token)
            )
        })?;

        match op {
            InflightOp::Accept(op) => {
                self.stats.cqe_accept += 1;
                self.accept_submitted = false;
                if result >= 0 {
                    let conn_fd = result as RawFd;
                    let remote_addr =
                        server_iouring::sockaddr_to_socket_addr(op.addr(), op.addr_len())?;
                    server_iouring::set_connection_options(conn_fd)?;
                    let conn_id = self.conn_id_alloc.fetch_add(1, Ordering::Relaxed);
                    let target_worker = self.worker.route_connection(conn_id, remote_addr);
                    if target_worker == self.worker.worker_index() {
                        self.register_connection(conn_id, conn_fd, remote_addr)?;
                    } else {
                        self.dispatch_mailbox_message(
                                target_worker,
                                WorkerMailboxMsg::AdoptConnection(
                                    crate::server_ur::transferred_connection::TransferredConnection::new(
                                        crate::server_ur::routing::ConnectionTransfer::new(
                                            conn_id,
                                            target_worker,
                                            crate::server_ur::fsm::ConnectionState::Accepted,
                                            remote_addr,
                                        ),
                                        conn_fd,
                                        Vec::new(),
                                        None,
                                    ),
                                ),
                            )?;
                    }
                }
            }
            InflightOp::MailboxRead { .. } => {
                self.stats.cqe_mailbox += 1;
                self.mailbox_read_submitted = false;
                self.drain_mailbox()?;
            }
            InflightOp::Recv { conn_id } => {
                self.stats.cqe_recv += 1;
                if let Some(connection) = self.connections.get_mut(&conn_id) {
                    connection.set_recv_inflight(false);
                    if result <= 0 {
                        self.submit_close_if_needed(conn_id)?;
                    } else {
                        let read = result as usize;
                        let chunk = connection.recv_slice(read).to_vec();
                        connection.read_buf_mut().extend_from_slice(&chunk);
                        self.drain_frames(conn_id)?;
                        if self.shutting_down {
                            self.submit_close_if_drained(conn_id)?;
                        } else {
                            self.queue_recv_if_needed(conn_id);
                        }
                    }
                }
            }
            InflightOp::Send { conn_id } => {
                self.stats.cqe_send += 1;
                if let Some(connection) = self.connections.get_mut(&conn_id) {
                    if let Some(mut inflight) = connection.take_send_inflight() {
                        if result <= 0 {
                            self.submit_close_if_needed(conn_id)?;
                        } else {
                            let written = result as usize;
                            if written < inflight.len() {
                                inflight.drain(0..written);
                                connection.set_send_inflight(Some(inflight));
                                self.queue_send_inflight(conn_id);
                            } else if !connection.pending_write().is_empty() {
                                let pending_write = connection.take_pending_write();
                                connection.set_send_inflight(Some(pending_write));
                                self.queue_send_inflight(conn_id);
                            } else if self.shutting_down {
                                self.submit_close_if_drained(conn_id)?;
                            }
                        }
                    }
                }
            }
            InflightOp::OpenFile(op) => {
                self.stats.cqe_log_open += 1;
                if result < 0 {
                    return Err(m_error!(
                        EC::IOErr,
                        format!("worker file open completion error {}", result)
                    ));
                }
                self.log
                    .finish_pending_open_file(op.request_id(), result as RawFd)?;
            }
            InflightOp::CloseFile(op) => {
                self.stats.cqe_file_close += 1;
                if result < 0 {
                    return Err(m_error!(
                        EC::IOErr,
                        format!("worker file close completion error {}", result)
                    ));
                }
                self.log.finish_pending_close_file(op.request_id())?;
            }
            InflightOp::LogWrite => {
                self.stats.cqe_log_write += 1;
                if result < 0 {
                    return Err(m_error!(
                        EC::IOErr,
                        format!("worker log write completion error {}", result)
                    ));
                }
                if let Some(mut inflight) = self.log.take_inflight_write() {
                    let written = result as usize;
                    if written < inflight.payload_len() {
                        inflight.consume_prefix(written);
                        self.log.set_inflight_write(Some(inflight));
                    } else {
                        let chunk_sequence = inflight.chunk_sequence();
                        drop(inflight);
                        self.log.cleanup_chunk_if_unused(chunk_sequence)?;
                    }
                }
            }
            InflightOp::Close { conn_id } => {
                self.stats.cqe_close += 1;
                self.worker.close_connection_sessions(conn_id)?;
                self.connections.remove(&conn_id);
            }
        }
        Ok(())
    }

    pub(in crate::server_ur) fn drain_procedure_completions(&mut self) {
        while let Some(op_id) = self.procedure_completion_queue.pop() {
            let Some(task_id) = self.procedure_op_registry.remove(&op_id) else {
                continue;
            };
            let Some(task) = self.procedure_tasks.get(&task_id) else {
                continue;
            };
            if !task.queued().swap(true, Ordering::AcqRel) {
                self.procedure_ready_queue.push(task_id);
            }
        }
    }

    pub(in crate::server_ur) fn poll_ready_procedures(&mut self) -> RS<()> {
        while let Some(task_id) = self.procedure_ready_queue.pop() {
            let Some(mut task) = self.procedure_tasks.remove(&task_id) else {
                continue;
            };
            // The task has left the ready queue and is about to be polled. A
            // later wake is required to enqueue it again.
            task.clear_queued();
            if let Some(waiting_on) = task.take_waiting_on() {
                self.procedure_op_registry.remove(&waiting_on);
            }
            let op_id = self.next_op_id;
            self.next_op_id += 1;

            // Each poll of a pending procedure installs a fresh worker-local
            // waker bound to a new op_id. Within that single pending interval,
            // repeated wakeups are coalesced by the waker so the task is
            // queued for one follow-up poll at most once. If the task remains
            // pending after that poll, the worker allocates a new op_id and a
            // new waker for the next interval.
            let waker = waker(Arc::new(ProcedureTaskWaker::new(
                op_id,
                self.procedure_completion_queue.clone(),
                task.completed().clone(),
            )));
            let mut cx = Context::from_waker(&waker);
            match task.future_mut().poll(&mut cx) {
                Poll::Ready(Ok(result)) => {
                    if let Some(connection) = self.connections.get_mut(&task.conn_id()) {
                        let response = encode_procedure_invoke_response(
                            task.request_id(),
                            &ProcedureInvokeResponse::new(result),
                        )?;
                        connection.extend_pending_write(&response);
                        self.queue_send_if_needed(task.conn_id());
                    }
                }
                Poll::Ready(Err(err)) => {
                    if let Some(connection) = self.connections.get_mut(&task.conn_id()) {
                        let response = encode_error_response(task.request_id(), err.to_string())?;
                        connection.extend_pending_write(&response);
                        self.queue_send_if_needed(task.conn_id());
                    }
                }
                Poll::Pending => {
                    // A pending future is modeled as a worker-local async
                    // operation. The op is not tied to the kernel ring itself,
                    // but it follows the same "op_id -> completion -> task
                    // resume" semantics so the worker loop owns progression.
                    task.set_waiting_on(op_id);
                    self.procedure_op_registry.insert(op_id, task_id);
                    self.procedure_tasks.insert(task_id, task);
                }
            }
        }
        Ok(())
    }

    pub(in crate::server_ur) fn drain_mailbox(&mut self) -> RS<()> {
        while let Some(msg) = self.mailbox.pop() {
            self.stats.mailbox_drained += 1;
            self.handle_mailbox_message(msg)?;
        }
        Ok(())
    }

    fn handle_mailbox_message(&mut self, msg: WorkerMailboxMsg) -> RS<()> {
        match msg {
            WorkerMailboxMsg::AdoptConnection(connection) => {
                server_iouring::set_connection_options(connection.fd())?;
                self.worker.adopt_connection_sessions(
                    connection.transfer().conn_id(),
                    connection.session_ids(),
                )?;
                self.register_connection(
                    connection.transfer().conn_id(),
                    connection.fd(),
                    connection.transfer().remote_addr(),
                )?;
                if let Some(action) = connection.session_open_action() {
                    let response = match self
                        .worker
                        .open_session_with_config(connection.transfer().conn_id(), action.config())
                    {
                        Ok(session_id) => encode_session_create_response(
                            action.request_id(),
                            &SessionCreateResponse::new(session_id),
                        )?,
                        Err(err) => encode_error_response(action.request_id(), err.to_string())?,
                    };
                    if let Some(conn) = self.connections.get_mut(&connection.transfer().conn_id()) {
                        conn.extend_pending_write(&response);
                    }
                    self.queue_send_if_needed(connection.transfer().conn_id());
                }
            }
            WorkerMailboxMsg::Shutdown => {
                self.shutdown_triggered = true;
            }
        }
        Ok(())
    }

    pub(in crate::server_ur) fn register_connection(
        &mut self,
        conn_id: u64,
        fd: RawFd,
        remote_addr: std::net::SocketAddr,
    ) -> RS<()> {
        self.stats.local_register += 1;
        self.connections
            .insert(conn_id, Box::new(WorkerConnection::new(fd, remote_addr)));
        self.queue_recv_if_needed(conn_id);
        Ok(())
    }

    pub(in crate::server_ur) fn submit_accept_if_needed(&mut self) -> RS<()> {
        if self.shutting_down || self.accept_submitted || self.listener_fd < 0 {
            return Ok(());
        }
        let sqe = unsafe { rliburing::io_uring_get_sqe(&mut self.ring) };
        if sqe.is_null() {
            return Ok(());
        }
        let mut op = Box::new(AcceptOp::new(
            unsafe { std::mem::zeroed() },
            std::mem::size_of::<rliburing::sockaddr_storage>() as rliburing::socklen_t,
        ));
        let token = self.alloc_token();
        unsafe {
            (*sqe).user_data = token;
            rliburing::io_uring_prep_accept(
                sqe,
                self.listener_fd,
                op.addr_mut_ptr(),
                op.addr_len_mut(),
                0,
            );
        }
        self.inflight.insert(token, InflightOp::Accept(op));
        self.accept_submitted = true;
        self.stats.accept_submit += 1;
        Ok(())
    }

    pub(in crate::server_ur) fn submit_mailbox_read_if_needed(&mut self) -> RS<()> {
        if self.mailbox_read_submitted || self.shutting_down {
            return Ok(());
        }
        let sqe = unsafe { rliburing::io_uring_get_sqe(&mut self.ring) };
        if sqe.is_null() {
            return Ok(());
        }
        let mut value = Box::new(0u64);
        let token = self.alloc_token();
        unsafe {
            (*sqe).user_data = token;
            rliburing::io_uring_prep_read(
                sqe,
                self.mailbox_fd,
                (&mut *value) as *mut u64 as *mut libc::c_void,
                std::mem::size_of::<u64>() as _,
                0,
            );
        }
        self.inflight
            .insert(token, InflightOp::MailboxRead { value });
        self.mailbox_read_submitted = true;
        self.stats.mailbox_submit += 1;
        Ok(())
    }

    pub(in crate::server_ur) fn submit_recv_ops(&mut self) -> RS<()> {
        if self.shutting_down {
            return Ok(());
        }
        while let Some(conn_id) = self.ready_recv.pop_front() {
            self.stats.recv_queue_pop += 1;
            let Some(connection) = self.connections.get_mut(&conn_id) else {
                continue;
            };
            connection.set_recv_ready_queued(false);
            if connection.recv_inflight() || connection.close_submitted() {
                continue;
            }
            let fd = connection.fd();
            let recv_ptr = connection.recv_buf_mut_ptr();
            let recv_len = connection.recv_buf_len();
            connection.set_recv_inflight(true);
            let _ = connection;
            let sqe = unsafe { rliburing::io_uring_get_sqe(&mut self.ring) };
            if sqe.is_null() {
                if let Some(connection) = self.connections.get_mut(&conn_id) {
                    connection.set_recv_inflight(false);
                    connection.set_recv_ready_queued(true);
                }
                self.ready_recv.push_front(conn_id);
                break;
            }
            let token = self.alloc_token();
            unsafe {
                (*sqe).user_data = token;
                rliburing::io_uring_prep_recv(
                    sqe,
                    fd,
                    recv_ptr as *mut libc::c_void,
                    recv_len as _,
                    0,
                );
            }
            self.inflight.insert(token, InflightOp::Recv { conn_id });
            self.stats.recv_submit += 1;
        }
        Ok(())
    }

    pub(in crate::server_ur) fn submit_send_ops(&mut self) -> RS<()> {
        while let Some(conn_id) = self.ready_send.pop_front() {
            self.stats.send_queue_pop += 1;
            let Some(connection) = self.connections.get_mut(&conn_id) else {
                continue;
            };
            connection.set_send_ready_queued(false);
            if connection.close_submitted() {
                continue;
            }
            if connection.send_inflight().is_some() {
                continue;
            }
            if !connection.pending_write().is_empty() {
                let pending_write = connection.take_pending_write();
                connection.set_send_inflight(Some(pending_write));
            }
            let Some(inflight) = connection.send_inflight() else {
                continue;
            };
            let fd = connection.fd();
            let send_ptr = inflight.as_ptr();
            let send_len = inflight.len();
            let _ = connection;
            let sqe = unsafe { rliburing::io_uring_get_sqe(&mut self.ring) };
            if sqe.is_null() {
                if let Some(connection) = self.connections.get_mut(&conn_id) {
                    connection.set_send_ready_queued(true);
                }
                self.ready_send.push_front(conn_id);
                break;
            }
            let token = self.alloc_token();
            unsafe {
                (*sqe).user_data = token;
                rliburing::io_uring_prep_send(
                    sqe,
                    fd,
                    send_ptr as *const libc::c_void,
                    send_len as _,
                    0,
                );
            }
            self.inflight.insert(token, InflightOp::Send { conn_id });
            self.stats.send_submit += 1;
        }
        Ok(())
    }

    pub(in crate::server_ur) fn submit_log_write_if_needed(&mut self) -> RS<()> {
        if self.shutting_down {
            return Ok(());
        }
        if self.log.inflight_write().is_none() {
            if let Some(pending) = self.log.take_pending_write() {
                self.log.set_inflight_write(Some(pending));
            }
        }
        let Some(inflight) = self.log.inflight_write() else {
            return Ok(());
        };
        let log_fd = inflight.fd();
        let payload_ptr = inflight.payload().as_ptr();
        let payload_len = inflight.payload_len();
        let payload_offset = inflight.offset();
        let sqe = unsafe { rliburing::io_uring_get_sqe(&mut self.ring) };
        if sqe.is_null() {
            return Ok(());
        }
        let token = self.alloc_token();
        unsafe {
            (*sqe).user_data = token;
            rliburing::io_uring_prep_write(
                sqe,
                log_fd,
                payload_ptr as *const libc::c_void,
                payload_len as _,
                payload_offset as _,
            );
        }
        self.inflight.insert(token, InflightOp::LogWrite);
        self.stats.log_write_submit += 1;
        Ok(())
    }

    pub(in crate::server_ur) fn submit_log_open_if_needed(&mut self) -> RS<()> {
        if self.shutting_down || self.log.inflight_open() {
            return Ok(());
        }
        let Some(request) = self.log.take_pending_open_file()? else {
            return Ok(());
        };
        let sqe = unsafe { rliburing::io_uring_get_sqe(&mut self.ring) };
        if sqe.is_null() {
            self.log.rollback_pending_open_file(request.request_id())?;
            return Ok(());
        }
        let token = self.alloc_token();
        let op = OpenFileOp::new(request);
        unsafe {
            (*sqe).user_data = token;
            rliburing::io_uring_prep_openat(
                sqe,
                libc::AT_FDCWD,
                op.request().path().as_ptr(),
                op.request().flags(),
                op.request().mode(),
            );
        }
        self.inflight
            .insert(token, InflightOp::OpenFile(Box::new(op)));
        self.stats.log_open_submit += 1;
        Ok(())
    }

    pub(in crate::server_ur) fn submit_file_close_if_needed(&mut self) -> RS<()> {
        if self.shutting_down || self.log.inflight_close() {
            return Ok(());
        }
        let Some(request) = self.log.take_pending_close_file() else {
            return Ok(());
        };
        let sqe = unsafe { rliburing::io_uring_get_sqe(&mut self.ring) };
        if sqe.is_null() {
            self.log.rollback_pending_close_file(request)?;
            return Ok(());
        }
        let token = self.alloc_token();
        let op = CloseFileOp::new(request);
        unsafe {
            (*sqe).user_data = token;
            rliburing::io_uring_prep_close(sqe, op.request().fd());
        }
        self.inflight
            .insert(token, InflightOp::CloseFile(Box::new(op)));
        self.stats.file_close_submit += 1;
        Ok(())
    }

    pub(in crate::server_ur) fn submit_close_if_needed(&mut self, conn_id: u64) -> RS<()> {
        let Some(connection) = self.connections.get_mut(&conn_id) else {
            return Ok(());
        };
        if connection.close_submitted() {
            return Ok(());
        }
        let fd = connection.fd();
        connection.set_close_submitted(true);
        let _ = connection;
        let sqe = unsafe { rliburing::io_uring_get_sqe(&mut self.ring) };
        if sqe.is_null() {
            if let Some(connection) = self.connections.get_mut(&conn_id) {
                connection.set_close_submitted(false);
            }
            return Ok(());
        }
        let token = self.alloc_token();
        unsafe {
            (*sqe).user_data = token;
            rliburing::io_uring_prep_close(sqe, fd);
        }
        self.inflight.insert(token, InflightOp::Close { conn_id });
        Ok(())
    }

    pub(in crate::server_ur) fn drain_frames(&mut self, conn_id: u64) -> RS<()> {
        loop {
            let Some(connection) = self.connections.get_mut(&conn_id) else {
                return Ok(());
            };
            let Some((frame, consumed)) =
                server_iouring::try_decode_next_frame(connection.read_buf())?
            else {
                return Ok(());
            };
            connection.read_buf_mut().drain(0..consumed);
            match server_iouring::dispatch_frame_iouring(&self.worker, conn_id, &frame) {
                Ok(FrameDispatch::Immediate(response)) => {
                    connection.extend_pending_write(&response)
                }
                Ok(FrameDispatch::Pending(pending)) => {
                    let task_id = self.next_task_id;
                    self.next_task_id += 1;
                    let (pending_conn_id, request_id, completed, future) = pending.into_parts();
                    self.procedure_tasks.insert(
                        task_id,
                        ProcedureTask::new(task_id, pending_conn_id, request_id, future, completed),
                    );
                    // Every new task is queued once for its initial poll. Any
                    // later poll must come from a completion-triggered wakeup.
                    self.procedure_ready_queue.push(task_id);
                }
                Ok(FrameDispatch::Transfer(transfer)) => {
                    let remote_addr = connection.remote_addr();
                    let fd = connection.fd();
                    self.connections.remove(&conn_id);
                    self.dispatch_mailbox_message(
                        transfer.target_worker(),
                        WorkerMailboxMsg::AdoptConnection(
                            crate::server_ur::transferred_connection::TransferredConnection::new(
                                crate::server_ur::routing::ConnectionTransfer::new(
                                    conn_id,
                                    transfer.target_worker(),
                                    crate::server_ur::fsm::ConnectionState::Active,
                                    remote_addr,
                                ),
                                fd,
                                transfer.session_ids().to_vec(),
                                Some(transfer.action()),
                            ),
                        ),
                    )?;
                    return Ok(());
                }
                Err(err) => {
                    let response =
                        encode_error_response(frame.header().request_id(), err.to_string())?;
                    connection.extend_pending_write(&response);
                }
            }
            self.queue_send_if_needed(conn_id);
        }
    }

    fn dispatch_mailbox_message(&self, target_worker: usize, msg: WorkerMailboxMsg) -> RS<()> {
        let Some(mailbox) = self.mailboxes.get(target_worker) else {
            return Err(m_error!(
                EC::InternalErr,
                format!("mailbox target worker {} is out of range", target_worker)
            ));
        };
        let Some(&fd) = self.mailbox_fds.get(target_worker) else {
            return Err(m_error!(
                EC::InternalErr,
                format!(
                    "mailbox eventfd target worker {} is out of range",
                    target_worker
                )
            ));
        };
        mailbox.push(msg);
        server_iouring::notify_mailbox_fd(fd)
    }

    pub(in crate::server_ur) fn alloc_token(&mut self) -> u64 {
        let token = self.next_token;
        self.next_token += 1;
        token
    }

    fn queue_recv_if_needed(&mut self, conn_id: u64) {
        if self.shutting_down {
            return;
        }
        let Some(connection) = self.connections.get_mut(&conn_id) else {
            return;
        };
        if connection.close_submitted()
            || connection.recv_inflight()
            || connection.recv_ready_queued()
        {
            return;
        }
        connection.set_recv_ready_queued(true);
        self.ready_recv.push_back(conn_id);
        self.stats.recv_queue_push += 1;
    }

    fn queue_send_if_needed(&mut self, conn_id: u64) {
        let Some(connection) = self.connections.get_mut(&conn_id) else {
            return;
        };
        if connection.close_submitted()
            || connection.send_inflight().is_some()
            || connection.pending_write().is_empty()
            || connection.send_ready_queued()
        {
            return;
        }
        connection.set_send_ready_queued(true);
        self.ready_send.push_back(conn_id);
        self.stats.send_queue_push += 1;
    }

    fn queue_send_inflight(&mut self, conn_id: u64) {
        let Some(connection) = self.connections.get_mut(&conn_id) else {
            return;
        };
        if connection.close_submitted()
            || connection.send_inflight().is_none()
            || connection.send_ready_queued()
        {
            return;
        }
        connection.set_send_ready_queued(true);
        self.ready_send.push_back(conn_id);
        self.stats.send_queue_push += 1;
    }

    fn begin_shutdown(&mut self) -> RS<()> {
        if self.shutting_down {
            return Ok(());
        }
        self.shutting_down = true;
        self.close_pending_mailbox_connections()?;
        self.ready_recv.clear();
        self.procedure_tasks.clear();
        self.procedure_op_registry.clear();
        while self.procedure_ready_queue.pop().is_some() {}
        while self.procedure_completion_queue.pop().is_some() {}
        if self.listener_fd >= 0 {
            let rc = unsafe { libc::close(self.listener_fd) };
            if rc != 0 {
                return Err(m_error!(
                    EC::NetErr,
                    "close io_uring listener during shutdown error",
                    std::io::Error::last_os_error()
                ));
            }
            self.listener_fd = -1;
        }
        self.submit_close_for_drained_connections()?;
        Ok(())
    }

    fn close_pending_mailbox_connections(&mut self) -> RS<()> {
        while let Some(msg) = self.mailbox.pop() {
            if let WorkerMailboxMsg::AdoptConnection(connection) = msg {
                let rc = unsafe { libc::close(connection.fd()) };
                if rc != 0 {
                    return Err(m_error!(
                        EC::NetErr,
                        "close transferred io_uring connection during shutdown error",
                        std::io::Error::last_os_error()
                    ));
                }
            }
        }
        Ok(())
    }

    fn submit_close_for_drained_connections(&mut self) -> RS<()> {
        let conn_ids: Vec<u64> = self.connections.keys().copied().collect();
        for conn_id in conn_ids {
            self.submit_close_if_drained(conn_id)?;
        }
        Ok(())
    }

    fn submit_close_if_drained(&mut self, conn_id: u64) -> RS<()> {
        let Some(connection) = self.connections.get(&conn_id) else {
            return Ok(());
        };
        if connection.send_inflight().is_some()
            || !connection.pending_write().is_empty()
            || !connection.read_buf().is_empty()
        {
            return Ok(());
        }
        self.submit_close_if_needed(conn_id)
    }

    fn finish_shutdown(&mut self) -> RS<WorkerLoopStats> {
        unsafe { rliburing::io_uring_queue_exit(&mut self.ring) };
        Ok(self.stats.clone())
    }
}
