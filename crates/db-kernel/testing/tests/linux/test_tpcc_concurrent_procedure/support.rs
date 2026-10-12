use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(super) struct TpccBenchCfg {
    pub(super) terminals: usize,
    pub(super) operations_per_terminal: usize,
    pub(super) warehouses: i32,
    pub(super) districts_per_warehouse: i32,
    pub(super) customers_per_district: i32,
    pub(super) items: i32,
    pub(super) stock_quantity: i32,
    pub(super) payment_percent: usize,
    pub(super) new_order_percent: usize,
    pub(super) trace_ops: bool,
    pub(super) treat_tx_errors_as_aborts: bool,
    pub(super) invoke_timeout_ms: u64,
    pub(super) bench_timeout_ms: u64,
    pub(super) app_name: String,
    mpk_path_env: Option<PathBuf>,
}

impl TpccBenchCfg {
    pub(super) fn from_env() -> Self {
        Self {
            terminals: read_env_usize("TPCC_TERMINALS", 2),
            operations_per_terminal: read_env_usize("TPCC_OPS_PER_TERMINAL", 2),
            warehouses: read_env_i32("TPCC_WAREHOUSES", 1),
            districts_per_warehouse: read_env_i32("TPCC_DISTRICTS_PER_WAREHOUSE", 2),
            customers_per_district: read_env_i32("TPCC_CUSTOMERS_PER_DISTRICT", 20),
            items: read_env_i32("TPCC_ITEMS", 64),
            stock_quantity: read_env_i32("TPCC_STOCK_QUANTITY", 100),
            payment_percent: read_env_usize("TPCC_PAYMENT_PERCENT", 43),
            new_order_percent: read_env_usize("TPCC_NEW_ORDER_PERCENT", 45),
            trace_ops: read_env_bool("TPCC_TRACE_OPS", false),
            treat_tx_errors_as_aborts: false,
            invoke_timeout_ms: read_env_u64("TPCC_INVOKE_TIMEOUT_MS", 30_000),
            bench_timeout_ms: read_env_u64("TPCC_BENCH_TIMEOUT_MS", 300_000),
            app_name: mudu_sys::env_var::var("TPCC_APP_NAME").unwrap_or_else(|| "tpcc".to_string()),
            mpk_path_env: mudu_sys::env_var::var("TPCC_MPK_PATH").map(PathBuf::from),
        }
    }

    pub(super) fn resolve_mpk_path(&self) -> Option<PathBuf> {
        if let Some(path) = &self.mpk_path_env {
            if path.exists() {
                return Some(path.clone());
            }
            return None;
        }

        let root = repo_root();
        let candidates = [
            root.join("crates")
                .join("db-kernel")
                .join("testing")
                .join("mpk")
                .join("tpcc.mpk"),
            root.join("target")
                .join("wasm32-wasip2")
                .join("release")
                .join("tpcc.mpk"),
            root.join("crates")
                .join("sdk")
                .join("example")
                .join("tpcc")
                .join("target")
                .join("wasm32-wasip2")
                .join("release")
                .join("tpcc.mpk"),
        ];
        candidates.into_iter().find(|path| path.exists())
    }
}

pub(super) fn read_env_usize(key: &str, default: usize) -> usize {
    mudu_sys::env_var::var(key)
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn read_env_i32(key: &str, default: i32) -> i32 {
    mudu_sys::env_var::var(key)
        .and_then(|v| v.parse::<i32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn read_env_u64(key: &str, default: u64) -> u64 {
    mudu_sys::env_var::var(key)
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

pub(super) fn read_env_u16(key: &str, default: u16) -> u16 {
    mudu_sys::env_var::var(key)
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn read_env_bool(key: &str, default: bool) -> bool {
    mudu_sys::env_var::var(key)
        .map(|v| match v.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "y" | "on" => true,
            "0" | "false" | "no" | "n" | "off" => false,
            _ => default,
        })
        .unwrap_or(default)
}

fn repo_root() -> PathBuf {
    // Walk up to the repository root (the `.project.home` marker): the
    // mpk candidates below span the db-kernel and sdk groups plus the
    // shared <repo-root>/target directory.
    let mut dir = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    loop {
        if dir.join(".project.home").exists() {
            return dir;
        }
        if !dir.pop() {
            panic!("no ancestor of CARGO_MANIFEST_DIR contains a .project.home marker");
        }
    }
}
