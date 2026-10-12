use crate::mode_sync::sync_topology_worker_ids;
use crate::topology::{
    build_partition_placement_sql, build_partition_rule_sql, effective_partition_count,
    load_async_topology, load_sync_topology, partition_index_for_warehouse, partition_ranges,
};
use crate::{Args, Workload};
use mudu_client::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_client::management::ServerTopology;
use mudu_sys::task::sync::spawn_thread_named;
use mududb::binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mududb::common::result::RS;
use mududb::contract::protocol::ClientRequest;
use mududb::contract::{sql_params, sql_stmt};
use mududb::error::ErrorCode::Thread;
use mududb::mudu_error;
use mududb::sys_interface::sync_api::{mudu_close, mudu_command, mudu_open_argv};
use mududb::types::datum::DatumDyn;
use tpcc::rust::procedure_common::{
    customer_name, district_name, item_name, require_positive, warehouse_name,
};

pub(crate) fn init_schema_sync(xid: u128, args: &Args) -> RS<()> {
    let topology = if args.warehouse_partitioned {
        let topology = load_sync_topology()?;
        execute_statement_sync(xid, &build_partition_rule_sql(args, topology.workers.len()))?;
        execute_statement_sync(xid, &build_partition_placement_sql(args, &topology)?)?;
        Some(topology)
    } else {
        None
    };
    execute_sql_script(xid, schema_sql(args))?;
    execute_sql_script(xid, include_str!("../../../sql/init.sql"))?;
    if args.workload == Workload::Seckill {
        for statement in build_seckill_setup_sql(args, topology.as_ref())? {
            execute_statement_sync(xid, &statement)?;
        }
    }
    if args.hot_rows_per_warehouse > 0 {
        for statement in build_hotspot_setup_sql(args) {
            execute_statement_sync(xid, &statement)?;
        }
    }
    Ok(())
}

pub(crate) async fn init_schema_tcp(
    client: &mut AsyncClientImpl,
    session_id: u128,
    args: &Args,
) -> RS<()> {
    // Rule, placement, and partitioned tables are all created client-side:
    // placement must exist before the partitioned tables so each worker
    // creates storage for the partitions it owns.
    let topology = if args.warehouse_partitioned {
        let topology = load_async_topology(&args.http_addr).await?;
        execute_statement_tcp(
            client,
            &args.app_name,
            &build_partition_rule_sql(args, topology.workers.len()),
        )
        .await?;
        execute_statement_tcp(
            client,
            &args.app_name,
            &build_partition_placement_sql(args, &topology)?,
        )
        .await?;
        execute_sql_script_tcp(client, &args.app_name, schema_sql(args)).await?;
        execute_sql_script_tcp(
            client,
            &args.app_name,
            include_str!("../../../sql/init.sql"),
        )
        .await?;
        Some(topology)
    } else {
        None
    };
    if args.workload == Workload::Seckill {
        for statement in build_seckill_setup_sql(args, topology.as_ref())? {
            execute_statement_tcp(client, &args.app_name, &statement).await?;
        }
    }
    if args.hot_rows_per_warehouse > 0 {
        for statement in build_hotspot_setup_sql(args) {
            execute_statement_tcp(client, &args.app_name, &statement).await?;
        }
    }
    let _ = session_id;
    Ok(())
}

pub(crate) fn execute_sql_script(xid: u128, sql_script: &str) -> RS<()> {
    for statement in split_sql_statements(sql_script) {
        execute_statement_sync(xid, &statement)?;
    }
    Ok(())
}

/// SQL statements creating the per-warehouse hotspot table and seeding its
/// K rows per warehouse (one INSERT per row; see the seckill seeding note).
fn build_hotspot_setup_sql(args: &Args) -> Vec<String> {
    let mut statements = Vec::new();
    if args.warehouse_partitioned {
        statements.push(
            "CREATE TABLE tpcc_hotspot (h_w_id INTEGER NOT NULL, h_id INTEGER NOT NULL, h_counter INTEGER NOT NULL, PRIMARY KEY (h_w_id, h_id)) PARTITION BY GLOBAL RULE r_tpcc_wh REFERENCES (h_w_id)".to_string(),
        );
    } else {
        statements.push(
            "CREATE TABLE tpcc_hotspot (h_w_id INTEGER NOT NULL, h_id INTEGER NOT NULL, h_counter INTEGER NOT NULL, PRIMARY KEY (h_w_id, h_id))".to_string(),
        );
    }
    for warehouse_id in 1..=args.warehouses {
        for hot_id in 1..=args.hot_rows_per_warehouse {
            statements.push(format!(
                "INSERT INTO tpcc_hotspot (h_w_id, h_id, h_counter) VALUES ({warehouse_id}, {hot_id}, 0)"
            ));
        }
    }
    statements
}

/// SQL statements creating the seckill tables (plus partition rule/placement
/// in partitioned runs) and seeding the flash-sale items with enough stock
/// to never deplete during a run.
fn build_seckill_setup_sql(args: &Args, topology: Option<&ServerTopology>) -> RS<Vec<String>> {
    let mut statements = Vec::new();
    if args.warehouse_partitioned {
        let topology = topology.ok_or_else(|| {
            mudu_error!(
                mududb::error::ErrorCode::InvalidState,
                "seckill partitioned setup requires server topology"
            )
        })?;
        let worker_count = topology.workers.len();
        let partition_count = args
            .partition_count
            .unwrap_or(worker_count)
            .max(1)
            .min(args.seckill_items.max(1) as usize);
        let ranges = partition_ranges(args.seckill_items, partition_count);
        let partitions = ranges
            .iter()
            .enumerate()
            .map(|(index, (start, end))| {
                format!("PARTITION p{} VALUES FROM ({start}) TO ({end})", index + 1)
            })
            .collect::<Vec<_>>()
            .join(", ");
        statements.push(format!(
            "CREATE PARTITION RULE r_seckill RANGE ({partitions})"
        ));
        let placements = (1..=ranges.len())
            .map(|index| {
                let worker = &topology.workers[(index - 1) % worker_count];
                format!("PARTITION p{index} ON WORKER {}", worker.worker_id)
            })
            .collect::<Vec<_>>()
            .join(", ");
        statements.push(format!(
            "CREATE PARTITION PLACEMENT FOR RULE r_seckill ({placements})"
        ));
        statements.push(
            "CREATE TABLE seckill_item (si_id INTEGER PRIMARY KEY, si_name TEXT NOT NULL, si_stock INTEGER NOT NULL, si_sold INTEGER NOT NULL, si_price INTEGER NOT NULL) PARTITION BY GLOBAL RULE r_seckill REFERENCES (si_id)"
                .to_string(),
        );
        statements.push(
            "CREATE TABLE seckill_order (so_item_id INTEGER NOT NULL, so_id INTEGER NOT NULL, so_user_id INTEGER NOT NULL, so_amount INTEGER NOT NULL, so_payload TEXT NOT NULL, PRIMARY KEY (so_item_id, so_id)) PARTITION BY GLOBAL RULE r_seckill REFERENCES (so_item_id)"
                .to_string(),
        );
    } else {
        statements.push(
            "CREATE TABLE seckill_item (si_id INTEGER PRIMARY KEY, si_name TEXT NOT NULL, si_stock INTEGER NOT NULL, si_sold INTEGER NOT NULL, si_price INTEGER NOT NULL)"
                .to_string(),
        );
        statements.push(
            "CREATE TABLE seckill_order (so_item_id INTEGER NOT NULL, so_id INTEGER NOT NULL, so_user_id INTEGER NOT NULL, so_amount INTEGER NOT NULL, so_payload TEXT NOT NULL, PRIMARY KEY (so_item_id, so_id))"
                .to_string(),
        );
    }
    const SECKILL_INITIAL_STOCK: i32 = 1_000_000;
    // One INSERT per row: a multi-row INSERT whose rows span several
    // partitions on the same worker currently persists only one of those
    // rows (while reporting the full affected_rows), which would silently
    // seed only a fraction of the items.
    for item_id in 1..=args.seckill_items {
        statements.push(format!(
            "INSERT INTO seckill_item (si_id, si_name, si_stock, si_sold, si_price) VALUES ({item_id}, 'promo-item-{item_id}', {SECKILL_INITIAL_STOCK}, 0, 100)"
        ));
    }
    Ok(statements)
}

pub(crate) fn execute_statement_sync(xid: u128, statement: &str) -> RS<()> {
    let _ = mudu_command(xid, sql_stmt!(&statement), sql_params!(&()))?;
    Ok(())
}

async fn execute_sql_script_tcp(
    client: &mut AsyncClientImpl,
    app_name: &str,
    sql_script: &str,
) -> RS<()> {
    for statement in split_sql_statements(sql_script) {
        execute_statement_tcp(client, app_name, &statement).await?;
    }
    Ok(())
}

async fn execute_statement_tcp(
    client: &mut AsyncClientImpl,
    app_name: &str,
    statement: &str,
) -> RS<()> {
    let _ = client
        .execute(ClientRequest::new(
            app_name.to_string(),
            statement.to_string(),
        ))
        .await?;
    Ok(())
}

pub(crate) fn schema_sql(args: &Args) -> &'static str {
    if args.warehouse_partitioned {
        include_str!("../../../sql/ddl_warehouse_partitioned.sql")
    } else {
        include_str!("../../../sql/ddl.sql")
    }
}

/// Rows bundled into one multi-row INSERT statement by the seed: large enough
/// to amortize per-statement overhead (adapter round trip, parse, bind),
/// small enough to keep the SQL text and parameter vectors modest.
const SEED_BATCH_ROWS: usize = 500;
/// Upper bound on seed threads. Each thread seeds a contiguous warehouse
/// range through its own session pinned to the worker owning that range.
const SEED_MAX_THREADS: usize = 16;

/// Accumulates rows and flushes them as multi-row INSERT statements
/// (`INSERT INTO t (cols) VALUES <tpl>, <tpl>, ...`) through `mudu_command`.
struct SeedBatch {
    prefix: &'static str,
    row_template: &'static str,
    params: Vec<Box<dyn DatumDyn>>,
    row_count: usize,
}

impl SeedBatch {
    fn new(prefix: &'static str, row_template: &'static str) -> Self {
        Self {
            prefix,
            row_template,
            params: Vec::new(),
            row_count: 0,
        }
    }

    fn push_row(&mut self, xid: u128, row: Vec<Box<dyn DatumDyn>>) -> RS<()> {
        self.params.extend(row);
        self.row_count += 1;
        if self.row_count >= SEED_BATCH_ROWS {
            self.flush(xid)?;
        }
        Ok(())
    }

    fn flush(&mut self, xid: u128) -> RS<()> {
        if self.row_count == 0 {
            return Ok(());
        }
        let rows = vec![self.row_template; self.row_count].join(", ");
        let statement = format!("{}{}", self.prefix, rows);
        mudu_command(xid, sql_stmt!(&statement), &self.params)?;
        self.params.clear();
        self.row_count = 0;
        Ok(())
    }
}

/// Keeps the generated i_price within the NUMERIC(6,2) column range.
fn seed_item_price(item_id: i32) -> i32 {
    ((item_id - 1) % 999 + 1) * 10
}

/// Seeds the shared item table used by the non-partitioned schema (item rows
/// carry no i_w_id there), once, before warehouse shards start.
fn seed_shared_items(xid: u128, args: &Args) -> RS<()> {
    let mut batch = SeedBatch::new(
        "INSERT INTO item (i_id, i_name, i_price) VALUES ",
        "(?, ?, ?)",
    );
    for item_id in 1..=args.items {
        batch.push_row(
            xid,
            vec![
                Box::new(item_id),
                Box::new(item_name(item_id)),
                Box::new(seed_item_price(item_id)),
            ],
        )?;
    }
    batch.flush(xid)
}

/// Seeds warehouses `warehouse_start..=warehouse_end` with the same per-table
/// row content as `tpcc_seed_inner`, using batched multi-row INSERTs.
fn seed_warehouse_range(
    xid: u128,
    args: &Args,
    warehouse_start: i32,
    warehouse_end: i32,
) -> RS<()> {
    if args.warehouse_partitioned {
        for warehouse_id in warehouse_start..=warehouse_end {
            let mut batch = SeedBatch::new(
                "INSERT INTO item (i_w_id, i_id, i_name, i_price) VALUES ",
                "(?, ?, ?, ?)",
            );
            for item_id in 1..=args.items {
                batch.push_row(
                    xid,
                    vec![
                        Box::new(warehouse_id),
                        Box::new(item_id),
                        Box::new(item_name(item_id)),
                        Box::new(seed_item_price(item_id)),
                    ],
                )?;
            }
            batch.flush(xid)?;
        }
    }
    for warehouse_id in warehouse_start..=warehouse_end {
        mudu_command(
            xid,
            sql_stmt!(&"INSERT INTO warehouse (w_id, w_name, w_tax, w_ytd) VALUES (?, ?, ?, 0)"),
            sql_params!(&(warehouse_id, warehouse_name(warehouse_id), warehouse_id % 7)),
        )?;
        for district_id in 1..=args.districts_per_warehouse {
            mudu_command(
                xid,
                sql_stmt!(
                    &"INSERT INTO district (d_id, d_w_id, d_name, d_tax, d_ytd, d_next_o_id, d_last_delivery_o_id) VALUES (?, ?, ?, ?, 0, 1, 0)"
                ),
                sql_params!(&(
                    district_id,
                    warehouse_id,
                    district_name(warehouse_id, district_id),
                    district_id % 9
                )),
            )?;
            let mut batch = SeedBatch::new(
                "INSERT INTO customer (c_id, c_d_id, c_w_id, c_first, c_last, c_discount, c_credit, c_balance, c_ytd_payment, c_payment_cnt, c_delivery_cnt, c_last_order_id) VALUES ",
                "(?, ?, ?, ?, ?, ?, ?, 0, 0, 0, 0, 0)",
            );
            for customer_id in 1..=args.customers_per_district {
                let (first, last) = customer_name(warehouse_id, district_id, customer_id);
                batch.push_row(
                    xid,
                    vec![
                        Box::new(customer_id),
                        Box::new(district_id),
                        Box::new(warehouse_id),
                        Box::new(first),
                        Box::new(last),
                        Box::new(customer_id % 5),
                        Box::new("GC".to_string()),
                    ],
                )?;
            }
            batch.flush(xid)?;
        }
    }
    for warehouse_id in warehouse_start..=warehouse_end {
        let mut batch = SeedBatch::new(
            "INSERT INTO stock (s_i_id, s_w_id, s_quantity, s_ytd, s_order_cnt, s_remote_cnt) VALUES ",
            "(?, ?, ?, 0, 0, 0)",
        );
        for item_id in 1..=args.items {
            batch.push_row(
                xid,
                vec![Box::new(item_id), Box::new(warehouse_id), Box::new(100_i32)],
            )?;
        }
        batch.flush(xid)?;
    }
    Ok(())
}

/// Splits `1..=warehouses` into up to `shard_count` contiguous inclusive
/// ranges.
pub(crate) fn warehouse_shard_ranges(warehouses: i32, shard_count: usize) -> Vec<(i32, i32)> {
    let count = shard_count.max(1).min(warehouses.max(1) as usize);
    let base = warehouses / count as i32;
    let remainder = warehouses % count as i32;
    let mut start = 1;
    let mut ranges = Vec::with_capacity(count);
    for index in 0..count {
        let size = base + i32::from((index as i32) < remainder);
        ranges.push((start, start + size - 1));
        start += size;
    }
    ranges
}

pub(crate) fn run_seed_sync(xid: u128, args: &Args) -> RS<()> {
    if args.workload == Workload::Seckill {
        // The flash-sale workload only uses the seckill tables (seeded in
        // init_schema_*); skip the full TPC-C dataset load.
        return Ok(());
    }
    require_positive("warehouse_count", args.warehouses)?;
    require_positive("district_count", args.districts_per_warehouse)?;
    require_positive("customer_count", args.customers_per_district)?;
    require_positive("item_count", args.items)?;
    if !args.warehouse_partitioned {
        seed_shared_items(xid, args)?;
    }
    let shard_ranges = warehouse_shard_ranges(args.warehouses, SEED_MAX_THREADS);
    if shard_ranges.len() <= 1 {
        let (warehouse_start, warehouse_end) = shard_ranges[0];
        return seed_warehouse_range(xid, args, warehouse_start, warehouse_end);
    }
    // Pin each seed thread's session to the worker owning its shard's first
    // warehouse so the shard's writes stay local to one server worker (a
    // shard straddling a partition boundary is still correct, just slightly
    // less local). Without multi-port the single listener is worker 0.
    let worker_ids = if args.tcp_multi_port {
        sync_topology_worker_ids(&load_sync_topology()?)?
    } else {
        vec![0]
    };
    let partition_ranges = partition_ranges(
        args.warehouses,
        effective_partition_count(args, worker_ids.len()),
    );
    let mut handles = Vec::with_capacity(shard_ranges.len());
    for (shard_index, (warehouse_start, warehouse_end)) in shard_ranges.iter().enumerate() {
        let worker_id = worker_ids
            [partition_index_for_warehouse(*warehouse_start, &partition_ranges) % worker_ids.len()];
        let shard_args = args.clone();
        let (warehouse_start, warehouse_end) = (*warehouse_start, *warehouse_end);
        handles.push(spawn_thread_named(
            format!("tpcc-seed-shard-{shard_index}"),
            move || {
                let shard_xid = mudu_open_argv(&UniSessionOpenArgv::new(worker_id))?;
                let seed_result =
                    seed_warehouse_range(shard_xid, &shard_args, warehouse_start, warehouse_end);
                let close_result = mudu_close(shard_xid);
                seed_result.and(close_result)
            },
        )?);
    }
    for handle in handles {
        handle
            .join()
            .map_err(|_| mudu_error!(Thread, "join tpcc seed shard thread error"))??;
    }
    Ok(())
}

fn split_sql_statements(sql_script: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut chars = sql_script.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                current.push(ch);
            }
            continue;
        }

        if in_block_comment {
            if ch == '*' && matches!(chars.peek(), Some('/')) {
                let _ = chars.next();
                in_block_comment = false;
            }
            continue;
        }

        if !in_single_quote && !in_double_quote {
            if ch == '-' && matches!(chars.peek(), Some('-')) {
                let _ = chars.next();
                in_line_comment = true;
                continue;
            }
            if ch == '/' && matches!(chars.peek(), Some('*')) {
                let _ = chars.next();
                in_block_comment = true;
                continue;
            }
        }

        if ch == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            current.push(ch);
            continue;
        }
        if ch == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            current.push(ch);
            continue;
        }

        if ch == ';' && !in_single_quote && !in_double_quote {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                statements.push(trimmed.to_string());
            }
            current.clear();
            continue;
        }

        current.push(ch);
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        statements.push(trimmed.to_string());
    }
    statements
}
