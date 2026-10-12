use crate::cli::{Args, Counters, Op, WorkerPhase};
use crate::progress::{ProgressTracker, mark_progress_completed, mark_progress_started};
use crate::util::{Lcg, choose_op, partitioned_user_key, sample_owned_key, shard_bounds};
use mududb::common::id::OID;
use mududb::common::result::RS;
use mududb::contract::database::sql_stmt_text::SQLStmtText;
use mududb::sys_interface::async_api::mudu_command as mudu_command_async;
use mududb::sys_interface::sync_api::mudu_command as mudu_command_sync;
use std::sync::atomic::{AtomicUsize, Ordering};
use ycsb::rust::procedure::{
    ycsb_insert, ycsb_read, ycsb_read_modify_write, ycsb_scan, ycsb_update,
};
use ycsb::rust::procedure_async::{
    ycsb_insert as ycsb_insert_async, ycsb_read as ycsb_read_async,
    ycsb_read_modify_write as ycsb_read_modify_write_async, ycsb_scan as ycsb_scan_async,
    ycsb_update as ycsb_update_async,
};

pub(crate) fn run_load_phase_sync(
    xid: OID,
    worker_idx: usize,
    connection_count: usize,
    partition_id: u128,
    args: &Args,
    value_template: &str,
    progress: Option<&ProgressTracker>,
) -> RS<()> {
    let (start, end) = shard_bounds(args.record_count, connection_count, worker_idx);
    for idx in start..end {
        mark_progress_started(progress, 1);
        let result = if transaction_enabled(args, WorkerPhase::Load) {
            run_in_transaction_sync(xid, || {
                ycsb_insert(
                    xid,
                    partitioned_user_key(partition_id, idx),
                    value_template.to_string(),
                )
            })
        } else {
            ycsb_insert(
                xid,
                partitioned_user_key(partition_id, idx),
                value_template.to_string(),
            )
        };
        result?;
        mark_progress_completed(progress, 1);
    }
    Ok(())
}

pub(crate) async fn run_load_phase_async(
    xid: OID,
    worker_idx: usize,
    connection_count: usize,
    partition_id: u128,
    args: &Args,
    value_template: &str,
    progress: Option<&ProgressTracker>,
) -> RS<()> {
    let (start, end) = shard_bounds(args.record_count, connection_count, worker_idx);
    for idx in start..end {
        mark_progress_started(progress, 1);
        let result = if transaction_enabled(args, WorkerPhase::Load) {
            run_in_transaction_async(xid, async {
                ycsb_insert_async(
                    xid,
                    partitioned_user_key(partition_id, idx),
                    value_template.to_string(),
                )
                .await
            })
            .await
        } else {
            ycsb_insert_async(
                xid,
                partitioned_user_key(partition_id, idx),
                value_template.to_string(),
            )
            .await
        };
        result?;
        mark_progress_completed(progress, 1);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_run_phase_sync(
    xid: OID,
    counters: &mut Counters,
    rng: &mut Lcg,
    worker_idx: usize,
    connection_count: usize,
    partition_id: u128,
    args: &Args,
    value_template: &str,
    next_insert: &AtomicUsize,
    progress: Option<&ProgressTracker>,
) -> RS<()> {
    let (start, end) = shard_bounds(args.operation_count, connection_count, worker_idx);
    let (key_start, key_end) = shard_bounds(args.record_count, connection_count, worker_idx);
    let mut owned_keys = (key_start..key_end).collect::<Vec<_>>();
    if owned_keys.is_empty() {
        owned_keys.push(key_start);
    }
    for _ in start..end {
        mark_progress_started(progress, 1);
        let op = choose_op(&args.workload, rng);
        let result = if transaction_enabled(args, WorkerPhase::Run) {
            run_in_transaction_sync(xid, || {
                execute_run_op_sync(
                    xid,
                    counters,
                    rng,
                    connection_count,
                    partition_id,
                    args,
                    value_template,
                    next_insert,
                    &mut owned_keys,
                    op,
                )
            })
        } else {
            execute_run_op_sync(
                xid,
                counters,
                rng,
                connection_count,
                partition_id,
                args,
                value_template,
                next_insert,
                &mut owned_keys,
                op,
            )
        };
        result?;
        mark_progress_completed(progress, 1);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_run_phase_async(
    xid: OID,
    counters: &mut Counters,
    rng: &mut Lcg,
    worker_idx: usize,
    connection_count: usize,
    partition_id: u128,
    args: &Args,
    value_template: &str,
    next_insert: &AtomicUsize,
    progress: Option<&ProgressTracker>,
) -> RS<()> {
    mudu_utils::scoped_task_trace!();
    let (start, end) = shard_bounds(args.operation_count, connection_count, worker_idx);
    let (key_start, key_end) = shard_bounds(args.record_count, connection_count, worker_idx);
    let mut owned_keys = (key_start..key_end).collect::<Vec<_>>();
    if owned_keys.is_empty() {
        owned_keys.push(key_start);
    }
    for _ in start..end {
        mark_progress_started(progress, 1);
        let op = choose_op(&args.workload, rng);
        let result = if transaction_enabled(args, WorkerPhase::Run) {
            run_in_transaction_async(
                xid,
                execute_run_op_async(
                    xid,
                    counters,
                    rng,
                    connection_count,
                    partition_id,
                    args,
                    value_template,
                    next_insert,
                    &mut owned_keys,
                    op,
                ),
            )
            .await
        } else {
            execute_run_op_async(
                xid,
                counters,
                rng,
                connection_count,
                partition_id,
                args,
                value_template,
                next_insert,
                &mut owned_keys,
                op,
            )
            .await
        };
        result?;
        mark_progress_completed(progress, 1);
    }
    Ok(())
}

fn transaction_enabled(args: &Args, phase: WorkerPhase) -> bool {
    args.enable_transaction && (matches!(phase, WorkerPhase::Run) || args.transaction_load)
}

fn begin_transaction_sync(xid: OID) -> RS<()> {
    exec_transaction_sql_sync(xid, "begin transaction")
}

fn commit_transaction_sync(xid: OID) -> RS<()> {
    exec_transaction_sql_sync(xid, "commit transaction")
}

fn rollback_transaction_sync(xid: OID) -> RS<()> {
    exec_transaction_sql_sync(xid, "rollback transaction")
}

fn exec_transaction_sql_sync(xid: OID, sql: &str) -> RS<()> {
    let stmt = SQLStmtText::new(sql.to_string());
    let _ = mudu_command_sync(xid as _, &stmt, &())?;
    Ok(())
}

async fn begin_transaction_async(xid: OID) -> RS<()> {
    exec_transaction_sql_async(xid, "begin transaction").await
}

async fn commit_transaction_async(xid: OID) -> RS<()> {
    exec_transaction_sql_async(xid, "commit transaction").await
}

async fn rollback_transaction_async(xid: OID) -> RS<()> {
    exec_transaction_sql_async(xid, "rollback transaction").await
}

async fn exec_transaction_sql_async(xid: OID, sql: &str) -> RS<()> {
    let stmt = SQLStmtText::new(sql.to_string());
    let _ = mudu_command_async(xid as _, &stmt, &()).await?;
    Ok(())
}

fn run_in_transaction_sync<F>(xid: OID, f: F) -> RS<()>
where
    F: FnOnce() -> RS<()>,
{
    begin_transaction_sync(xid)?;
    match f() {
        Ok(()) => commit_transaction_sync(xid),
        Err(err) => {
            let _ = rollback_transaction_sync(xid);
            Err(err)
        }
    }
}

async fn run_in_transaction_async<F>(xid: OID, future: F) -> RS<()>
where
    F: std::future::Future<Output = RS<()>>,
{
    begin_transaction_async(xid).await?;
    match future.await {
        Ok(()) => commit_transaction_async(xid).await,
        Err(err) => {
            let _ = rollback_transaction_async(xid).await;
            Err(err)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_run_op_sync(
    xid: OID,
    counters: &mut Counters,
    rng: &mut Lcg,
    connection_count: usize,
    partition_id: u128,
    args: &Args,
    value_template: &str,
    next_insert: &AtomicUsize,
    owned_keys: &mut Vec<usize>,
    op: Op,
) -> RS<()> {
    match op {
        Op::Read => {
            let key_index = sample_owned_key(owned_keys, rng);
            let key = partitioned_user_key(partition_id, key_index);
            let _ = ycsb_read(xid, key)?;
            counters.read += 1;
        }
        Op::Update => {
            let key_index = sample_owned_key(owned_keys, rng);
            let key = partitioned_user_key(partition_id, key_index);
            ycsb_update(xid, key, value_template.to_string())?;
            counters.update += 1;
        }
        Op::Insert => {
            let key_index = next_insert.fetch_add(connection_count, Ordering::Relaxed);
            owned_keys.push(key_index);
            ycsb_insert(
                xid,
                partitioned_user_key(partition_id, key_index),
                value_template.to_string(),
            )?;
            counters.insert += 1;
        }
        Op::Scan => {
            let scan_start = sample_owned_key(owned_keys, rng);
            let scan_end = scan_start.saturating_add(args.scan_length);
            let _ = ycsb_scan(
                xid,
                partitioned_user_key(partition_id, scan_start),
                partitioned_user_key(partition_id, scan_end),
            )?;
            counters.scan += 1;
        }
        Op::ReadModifyWrite => {
            let key_index = sample_owned_key(owned_keys, rng);
            let key = partitioned_user_key(partition_id, key_index);
            let _ = ycsb_read_modify_write(xid, key, "x".to_string())?;
            counters.rmw += 1;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn execute_run_op_async(
    xid: OID,
    counters: &mut Counters,
    rng: &mut Lcg,
    connection_count: usize,
    partition_id: u128,
    args: &Args,
    value_template: &str,
    next_insert: &AtomicUsize,
    owned_keys: &mut Vec<usize>,
    op: Op,
) -> RS<()> {
    match op {
        Op::Read => {
            let key_index = sample_owned_key(owned_keys, rng);
            let key = partitioned_user_key(partition_id, key_index);
            let _ = ycsb_read_async(xid, key).await?;
            counters.read += 1;
        }
        Op::Update => {
            let key_index = sample_owned_key(owned_keys, rng);
            let key = partitioned_user_key(partition_id, key_index);
            ycsb_update_async(xid, key, value_template.to_string()).await?;
            counters.update += 1;
        }
        Op::Insert => {
            let key_index = next_insert.fetch_add(connection_count, Ordering::Relaxed);
            owned_keys.push(key_index);
            ycsb_insert_async(
                xid,
                partitioned_user_key(partition_id, key_index),
                value_template.to_string(),
            )
            .await?;
            counters.insert += 1;
        }
        Op::Scan => {
            let scan_start = sample_owned_key(owned_keys, rng);
            let scan_end = scan_start.saturating_add(args.scan_length);
            let _ = ycsb_scan_async(
                xid,
                partitioned_user_key(partition_id, scan_start),
                partitioned_user_key(partition_id, scan_end),
            )
            .await?;
            counters.scan += 1;
        }
        Op::ReadModifyWrite => {
            let key_index = sample_owned_key(owned_keys, rng);
            let key = partitioned_user_key(partition_id, key_index);
            let _ = ycsb_read_modify_write_async(xid, key, "x".to_string()).await?;
            counters.rmw += 1;
        }
    }
    Ok(())
}
