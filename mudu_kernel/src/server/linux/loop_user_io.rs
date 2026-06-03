use std::collections::HashMap;

use mudu::common::result::RS;

use mudu_sys::io::worker_ring::{complete_user_ring_op, submit_user_ring_op, WorkerLocalRing};
use crate::server::inflight_op::InflightOp;

pub(in crate::server) struct LoopUserIoCtx<'a> {
    pub ring: &'a mut mudu_sys::uring::IoUring,
    pub user_ring: &'a WorkerLocalRing,
    pub inflight: &'a mut HashMap<u64, InflightOp>,
    pub next_token: &'a mut u64,
}

pub(in crate::server) fn submit(ctx: &mut LoopUserIoCtx<'_>) -> RS<()> {
    loop {
        let Some((op_id, op)) = ctx.user_ring.take_pending()? else {
            return Ok(());
        };
        let Some(mut sqe) = ctx.ring.next_sqe() else {
            ctx.user_ring.requeue_front(op_id, op)?;
            return Ok(());
        };
        let token = alloc_token(ctx.next_token);
        sqe.set_user_data(token);
        let inflight = submit_user_ring_op(op_id, op, &mut sqe);
        ctx.inflight.insert(token, InflightOp::UserIo(inflight));
    }
}

pub(in crate::server) fn handle_completion(
    user_ring: &WorkerLocalRing,
    op: mudu_sys::io::worker_ring::UserIoInflight,
    result: i32,
) -> RS<()> {
    complete_user_ring_op(op, result, user_ring)
}

fn alloc_token(next_token: &mut u64) -> u64 {
    let token = *next_token;
    *next_token += 1;
    token
}
