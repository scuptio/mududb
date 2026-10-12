use clap::Parser;

#[derive(Parser, Debug, Clone)]
pub(crate) struct Args {
    #[arg(long, default_value = "a")]
    pub(crate) workload: String,
    #[arg(long, default_value_t = 10_000)]
    pub(crate) record_count: usize,
    #[arg(long, default_value_t = 10_000)]
    pub(crate) operation_count: usize,
    #[arg(long, default_value_t = 256)]
    pub(crate) field_length: usize,
    #[arg(long, default_value_t = 100)]
    pub(crate) scan_length: usize,
    #[arg(long, default_value_t = 0)]
    pub(crate) seed: u64,
    #[arg(long, default_value_t = 1)]
    pub(crate) connection_count: usize,
    #[arg(long, default_value_t = 1)]
    pub(crate) partition_count: usize,
    #[arg(long, default_value_t = false)]
    pub(crate) enable_async: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) enable_transaction: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) transaction_load: bool,
    #[arg(long)]
    pub(crate) debug_http_port: Option<u16>,
}

#[derive(Clone, Copy)]
pub(crate) enum Op {
    Read,
    Update,
    Insert,
    Scan,
    ReadModifyWrite,
}

#[derive(Clone, Copy)]
pub(crate) enum WorkerPhase {
    Load,
    Run,
}

#[derive(Default)]
pub(crate) struct Counters {
    pub(crate) read: usize,
    pub(crate) update: usize,
    pub(crate) insert: usize,
    pub(crate) scan: usize,
    pub(crate) rmw: usize,
}

impl Counters {
    pub(crate) fn merge(&mut self, other: Counters) {
        self.read += other.read;
        self.update += other.update;
        self.insert += other.insert;
        self.scan += other.scan;
        self.rmw += other.rmw;
    }
}

impl WorkerPhase {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Load => "load",
            Self::Run => "run",
        }
    }
}
