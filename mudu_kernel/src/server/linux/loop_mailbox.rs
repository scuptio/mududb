use std::collections::HashMap;

use crossbeam_queue::SegQueue;
use mudu::common::result::RS;
use tracing::debug;

use crate::server::inflight_op::InflightOp;
use crate::server::worker_loop_stats::WorkerLoopStats;
use crate::server::worker_mailbox::WorkerMailboxMsg;

pub(in crate::server) struct LoopMailboxSubmitCtx<'a> {
    pub worker_id: u128,
    pub ring: &'a mut mudu_sys::uring::IoUring,
    pub mailbox_fd: i32,
    pub mailbox_read_submitted: &'a mut bool,
    pub inflight: &'a mut HashMap<u64, InflightOp>,
    pub next_token: &'a mut u64,
    pub stats: &'a mut WorkerLoopStats,
    pub shutting_down: bool,
}

pub(in crate::server) fn drain_messages(
    mailbox: &SegQueue<WorkerMailboxMsg>,
    stats: &mut WorkerLoopStats,
) -> Vec<WorkerMailboxMsg> {
    let mut drained = Vec::new();
    while let Some(msg) = mailbox.pop() {
        stats.mailbox_drained += 1;
        drained.push(msg);
    }
    debug!(count = drained.len(), "loop_mailbox drained messages");
    drained
}

pub(in crate::server) fn submit_read_if_needed(ctx: &mut LoopMailboxSubmitCtx<'_>) -> RS<()> {
    if *ctx.mailbox_read_submitted || ctx.shutting_down {
        debug!(
            worker_id = ctx.worker_id,
            mailbox_fd = ctx.mailbox_fd,
            mailbox_read_submitted = *ctx.mailbox_read_submitted,
            shutting_down = ctx.shutting_down,
            "loop_mailbox skip submit mailbox read"
        );
        return Ok(());
    }
    let Some(mut sqe) = ctx.ring.next_sqe() else {
        return Ok(());
    };
    let mut value = Box::new(0u64);
    let token = alloc_token(ctx.next_token);
    sqe.set_user_data(token);
    sqe.prep_read_raw(
        ctx.mailbox_fd,
        (&mut *value as *mut u64).cast(),
        std::mem::size_of::<u64>(),
        0,
    );
    ctx.inflight
        .insert(token, InflightOp::MailboxRead { _value: value });
    *ctx.mailbox_read_submitted = true;
    ctx.stats.mailbox_submit += 1;
    debug!(
        mailbox_fd = ctx.mailbox_fd,
        token, "loop_mailbox submit mailbox read"
    );
    debug!(
        worker_id = ctx.worker_id,
        mailbox_fd = ctx.mailbox_fd,
        token,
        "loop_mailbox submit mailbox read"
    );
    Ok(())
}

pub(in crate::server) fn handle_read_completion(
    worker_id: u128,
    mailbox_fd: i32,
    mailbox_read_submitted: &mut bool,
    stats: &mut WorkerLoopStats,
) {
    stats.cqe_mailbox += 1;
    *mailbox_read_submitted = false;
    debug!(
        worker_id,
        mailbox_fd, "loop_mailbox mailbox read completion"
    );
}

fn alloc_token(next_token: &mut u64) -> u64 {
    let token = *next_token;
    *next_token += 1;
    token
}
