#[derive(Debug, Default, Clone)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(in crate::server) struct WorkerLoopStats {
    pub worker_id: usize,
    pub submit_calls: u64,
    pub wait_cqe_calls: u64,
    pub cqe_accept: u64,
    pub cqe_mailbox: u64,
    pub cqe_recv: u64,
    pub cqe_send: u64,
    #[allow(dead_code)]
    pub cqe_log_open: u64,
    #[allow(dead_code)]
    pub cqe_file_close: u64,
    pub cqe_log_write: u64,
    pub cqe_close: u64,
    pub recv_queue_push: u64,
    pub recv_queue_pop: u64,
    pub send_queue_push: u64,
    pub send_queue_pop: u64,
    pub recv_submit: u64,
    pub send_submit: u64,
    #[allow(dead_code)]
    pub log_open_submit: u64,
    #[allow(dead_code)]
    pub file_close_submit: u64,
    pub log_write_submit: u64,
    pub accept_submit: u64,
    pub mailbox_submit: u64,
    pub mailbox_drained: u64,
    pub local_register: u64,
}
