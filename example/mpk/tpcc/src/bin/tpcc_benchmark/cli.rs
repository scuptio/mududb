use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum BenchmarkMode {
    Interactive,
    StoredProcedure,
    /// PostgreSQL stored procedures: each transaction runs as a single
    /// `SELECT tpcc_*()` call invoking a PL/pgSQL function installed into the
    /// target database (see sql/procedures_postgres.sql).
    PgProcedure,
}

/// Workload driver: the original TPC-C transaction mix or the write-heavy
/// flash-sale (seckill) mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Workload {
    Tpcc,
    Seckill,
}

#[derive(Parser, Debug, Clone)]
pub(crate) struct Args {
    #[arg(long, value_enum, default_value_t = BenchmarkMode::Interactive)]
    pub(crate) mode: BenchmarkMode,
    #[arg(long, default_value_t = 1)]
    pub(crate) warehouses: i32,
    #[arg(long, default_value_t = 10)]
    pub(crate) districts_per_warehouse: i32,
    #[arg(long, default_value_t = 100)]
    pub(crate) customers_per_district: i32,
    #[arg(long, default_value_t = 100)]
    pub(crate) items: i32,
    #[arg(long, default_value_t = 100)]
    pub(crate) operation_count: usize,
    /// Extra operations per phase executed before measurement; their results
    /// are excluded from the stats and the txn clock starts after them.
    #[arg(long, default_value_t = 0)]
    pub(crate) warmup_operations: usize,
    #[arg(long, default_value_t = 1)]
    pub(crate) connection_count: usize,
    #[arg(long, default_value_t = 50)]
    pub(crate) payment_percent: usize,
    #[arg(long, default_value_t = 35)]
    pub(crate) new_order_percent: usize,
    #[arg(long, default_value_t = false)]
    pub(crate) enable_async: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) warehouse_partitioned: bool,
    /// Number of range partitions for warehouse-partitioned runs. Defaults to
    /// the server worker count; must be between 1 and 50 (the whole partition
    /// rule is stored as a single catalog row that must fit one 4 KiB page).
    #[arg(long)]
    pub(crate) partition_count: Option<usize>,
    #[arg(long, default_value = "tpcc")]
    pub(crate) app_name: String,
    #[arg(long, default_value = "127.0.0.1:9527")]
    pub(crate) tcp_addr: String,
    #[arg(long, default_value = "127.0.0.1:8300")]
    pub(crate) http_addr: String,
    #[arg(long, default_value_t = false)]
    pub(crate) tcp_multi_port: bool,
    #[arg(long)]
    pub(crate) mpk: Option<PathBuf>,
    /// End-to-end perf sampling rate (0 = disabled, N = sample 1/N requests).
    /// Prints a per-request client/server stage breakdown to stderr.
    #[arg(long, default_value_t = 0)]
    pub(crate) perf_sample_rate: u64,
    /// Workload driver: original TPC-C mix (default) or write-heavy flash sale.
    #[arg(long, value_enum, default_value_t = Workload::Tpcc)]
    pub(crate) workload: Workload,
    /// Number of flash-sale items; ids are spread across partitions.
    #[arg(long, default_value_t = 320)]
    pub(crate) seckill_items: i32,
    /// Order payload size in bytes for the flash-sale workload.
    #[arg(long, default_value_t = 2048)]
    pub(crate) seckill_payload_bytes: usize,
    /// Percentage of flash-sale operations (0-100) routed to one hot item;
    /// 0 = uniform round-robin over all items. When partitioned, the hot
    /// item is the first item of the worker's owned range, so hotspot buys
    /// stay partition-local.
    #[arg(long, default_value_t = 0)]
    pub(crate) seckill_hotspot_percent: u32,
    /// Hot-row contention injector: number of hotspot rows per warehouse
    /// (0 = off, original TPC-C). When > 0, each op is followed by a
    /// `tpcc_hotspot_hit` transaction on one of the warehouse's K rows.
    #[arg(long, default_value_t = 0)]
    pub(crate) hot_rows_per_warehouse: i32,
    /// Fixed order-line count per new-order (0 = original 3-7 variable mix).
    #[arg(long, default_value_t = 0)]
    pub(crate) order_lines: usize,
    /// Fixed per-terminal think time in milliseconds, slept between
    /// transactions (0 = disabled). Excluded from per-op latency; included in
    /// wall-clock elapsed time, so it paces the offered load. The default is
    /// the TPC-C mix-weighted mean think time: the spec (Clause 5.2.5.4)
    /// defines per-type mean think times of 12s (New-Order, Payment), 10s
    /// (Order-Status), and 5s (Delivery, Stock-Level), which under this
    /// driver's default mix (35% New-Order, 50% Payment, 8% Delivery, 7%
    /// Stock-Level) averages to ~11s.
    #[arg(long, default_value_t = 11000)]
    pub(crate) think_time_ms: u64,
}

#[derive(Clone, Copy)]
pub(crate) enum TpccOp {
    NewOrder,
    Payment,
    OrderStatus,
    Delivery,
    StockLevel,
    SeckillBuy,
}

impl Args {
    pub(crate) fn proc_name(&self, proc_name: &str) -> String {
        let suffix = if self.warehouse_partitioned {
            format!("{proc_name}_partitioned")
        } else {
            proc_name.to_string()
        };
        format!("{}/tpcc/{}", self.app_name, suffix)
    }
}
