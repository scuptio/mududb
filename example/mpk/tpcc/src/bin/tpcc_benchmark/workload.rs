use crate::topology::{effective_partition_count, partition_index_for_warehouse, partition_ranges};
use crate::{Args, TpccOp, Workload};
use std::time::Duration;

pub(crate) fn op_for(index: usize, args: &Args) -> TpccOp {
    if args.workload == Workload::Seckill {
        return TpccOp::SeckillBuy;
    }
    let bucket = index % 100;
    if bucket < args.new_order_percent {
        TpccOp::NewOrder
    } else if bucket < args.new_order_percent + args.payment_percent {
        TpccOp::Payment
    } else if bucket < 85 {
        TpccOp::OrderStatus
    } else if bucket < 93 {
        TpccOp::Delivery
    } else {
        TpccOp::StockLevel
    }
}

pub(crate) fn value_for(index: usize, modulo: i32) -> i32 {
    (index as i32 % modulo) + 1
}

/// Per-terminal think time slept between transactions; `None` when disabled.
pub(crate) fn think_time(args: &Args) -> Option<Duration> {
    (args.think_time_ms > 0).then(|| Duration::from_millis(args.think_time_ms))
}

#[allow(dead_code)]
fn new_order_lines(
    index: usize,
    warehouse_id: i32,
    warehouse_count: i32,
    item_count: i32,
    local_only: bool,
) -> (Vec<i32>, Vec<i32>, Vec<i32>) {
    new_order_lines_with_count(
        index,
        warehouse_id,
        warehouse_count,
        item_count,
        local_only,
        0,
    )
}

#[allow(clippy::manual_is_multiple_of)]
pub(crate) fn new_order_lines_with_count(
    index: usize,
    warehouse_id: i32,
    warehouse_count: i32,
    item_count: i32,
    local_only: bool,
    fixed_line_count: usize,
) -> (Vec<i32>, Vec<i32>, Vec<i32>) {
    let line_count = if fixed_line_count > 0 {
        fixed_line_count
    } else {
        (index % 5) + 3
    };
    let mut item_ids = Vec::with_capacity(line_count);
    let mut supplier_warehouse_ids = Vec::with_capacity(line_count);
    let mut quantities = Vec::with_capacity(line_count);
    for line_idx in 0..line_count {
        item_ids.push(value_for(index * 7 + line_idx * 3 + 1, item_count));
        // TPC-C: about 1% of order-lines are supplied by a remote warehouse.
        // (Previously every third line was remote, which is far above the
        // spec and unfairly taxes partitioned backends.)
        let supplier_warehouse_id =
            if !local_only && warehouse_count > 1 && index % 25 == 0 && line_idx == 0 {
                value_for(index + line_idx + 1, warehouse_count)
            } else {
                warehouse_id
            };
        supplier_warehouse_ids.push(supplier_warehouse_id);
        quantities.push(((index + line_idx) % 10) as i32 + 1);
    }
    (item_ids, supplier_warehouse_ids, quantities)
}

pub(crate) fn warehouse_for_op(op_index: usize, terminal_id: usize, args: &Args) -> i32 {
    if !args.warehouse_partitioned {
        return value_for(op_index, args.warehouses);
    }
    value_for(terminal_id, args.warehouses)
}

/// Mirrors the terminal→worker mapping used by the workload loops
/// (run_sync_workload / run_tcp_multi_port).
#[allow(dead_code)]
fn terminal_worker_index(args: &Args, terminal_id: usize, worker_count: usize) -> usize {
    if args.warehouse_partitioned && worker_count > 0 {
        let ranges = partition_ranges(
            args.warehouses,
            effective_partition_count(args, worker_count),
        );
        partition_index_for_warehouse(value_for(terminal_id, args.warehouses), &ranges)
            % worker_count
    } else {
        terminal_id % worker_count.max(1)
    }
}

pub(crate) fn seckill_op_args(
    args: &Args,
    op_index: usize,
    user_id: i32,
    worker_index: usize,
    worker_count: usize,
) -> (i32, i32, i32, i32, String) {
    let item_id = seckill_item_for_worker(args, op_index, worker_index, worker_count);
    let order_id = (op_index + 1) as i32;
    let payload = "x".repeat(args.seckill_payload_bytes);
    (item_id, order_id, user_id, 100, payload)
}

/// Deterministic hot/normal decision for the seckill hotspot knob: hashes
/// `op_index` into [0, 100) so the hit ratio stays ~uniform under the
/// per-terminal stride (`step_by(connection_count)`), without an RNG.
fn seckill_op_is_hot(args: &Args, op_index: usize) -> bool {
    args.seckill_hotspot_percent > 0
        && (op_index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) % 100
            < args.seckill_hotspot_percent as u64
}

/// Picks a seckill item id owned by `worker_index`'s partitions, so buys stay
/// partition-local: placement maps partition j to worker j % worker_count
/// (see build_seckill_setup_sql). A buy issued on the wrong worker would
/// read a remote partition, observe no row, and degrade into a sold-out
/// no-op (which measures nothing).
fn seckill_item_for_worker(
    args: &Args,
    op_index: usize,
    worker_index: usize,
    worker_count: usize,
) -> i32 {
    if !args.warehouse_partitioned || worker_count == 0 {
        return if seckill_op_is_hot(args, op_index) {
            1
        } else {
            value_for(op_index, args.seckill_items)
        };
    }
    let partition_count = args
        .partition_count
        .unwrap_or(worker_count)
        .max(1)
        .min(args.seckill_items.max(1) as usize);
    let ranges = partition_ranges(args.seckill_items, partition_count);
    let owned: Vec<(i32, i32)> = ranges
        .iter()
        .enumerate()
        .filter(|(index, _)| index % worker_count == worker_index % worker_count)
        .map(|(_, range)| *range)
        .collect();
    if owned.is_empty() {
        return value_for(op_index, args.seckill_items);
    }
    if seckill_op_is_hot(args, op_index) {
        // Hot item: first item of the worker's first owned range, so hotspot
        // buys stay partition-local and contend on one row per worker.
        return owned[0].0;
    }
    let (start, end) = owned[(op_index / 8) % owned.len()];
    start + (op_index as i32 % (end - start).max(1))
}
