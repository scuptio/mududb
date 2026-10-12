/// Cumulative counters of one io_uring worker event loop, logged at worker
/// shutdown. Only populated on Linux; other platforms keep the zero value.
#[derive(Debug, Default, Clone)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct WorkerLoopStats {
    /// Worker index these counters belong to.
    pub worker_id: usize,
    /// `ring.submit()` calls.
    pub submit_calls: u64,
    /// Blocking CQE wait calls.
    pub wait_cqe_calls: u64,
    /// Completed accept operations.
    pub cqe_accept: u64,
    /// Completed mailbox-message operations.
    pub cqe_mailbox: u64,
    /// Completed receive operations.
    pub cqe_recv: u64,
    /// Completed send operations.
    pub cqe_send: u64,
    /// Completed log-write operations.
    pub cqe_log_write: u64,
    /// Completed close operations.
    pub cqe_close: u64,
    /// Pushes onto the receive queue.
    pub recv_queue_push: u64,
    /// Pops from the receive queue.
    pub recv_queue_pop: u64,
    /// Pushes onto the send queue.
    pub send_queue_push: u64,
    /// Pops from the send queue.
    pub send_queue_pop: u64,
    /// Submitted receive operations.
    pub recv_submit: u64,
    /// Submitted send operations.
    pub send_submit: u64,
    /// Submitted log-write operations.
    pub log_write_submit: u64,
    /// Submitted accept operations.
    pub accept_submit: u64,
    /// Submitted mailbox operations.
    pub mailbox_submit: u64,
    /// Drained mailbox messages.
    pub mailbox_drained: u64,
    /// Connections registered on this worker.
    pub local_register: u64,
}
