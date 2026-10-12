use crate::Args;
use mudu_sys::time::{Instant, instant_now};
use std::cell::Cell;

thread_local! {
    /// Timestamp of the moment the current operation's request was issued
    /// (set by `invoke_typed` right before awaiting the response). Lets the
    /// per-op loop split client-side prep from wire+server wait without
    /// threading a timing out-param through every op helper.
    pub(crate) static OP_SENT: Cell<Option<Instant>> = const { Cell::new(None) };

    /// Set by the seckill op handlers when the item was sold out; read by the
    /// op-result builder (same thread-local pattern as OP_SENT).
    pub(crate) static OP_SOLD_OUT: Cell<bool> = const { Cell::new(false) };
}

#[derive(Debug, Clone)]
pub(crate) struct OpResult {
    latency_ms: f64,
    prep_ms: f64,
    wait_ms: f64,
    aborted: bool,
    sold_out: bool,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct BenchmarkStats {
    results: Vec<OpResult>,
}

impl BenchmarkStats {
    pub(crate) fn push(&mut self, result: OpResult) {
        self.results.push(result);
    }

    pub(crate) fn merge(&mut self, other: BenchmarkStats) {
        self.results.extend(other.results);
    }

    fn op_count(&self) -> usize {
        self.results.len()
    }

    fn abort_count(&self) -> usize {
        self.results.iter().filter(|r| r.aborted).count()
    }

    fn abort_rate(&self) -> f64 {
        if self.results.is_empty() {
            0.0
        } else {
            self.abort_count() as f64 / self.results.len() as f64 * 100.0
        }
    }

    fn sold_out_count(&self) -> usize {
        self.results.iter().filter(|r| r.sold_out).count()
    }

    fn latency_percentile(&self, p: f64) -> f64 {
        percentile_of(self.results.iter().map(|r| r.latency_ms), p)
    }

    fn prep_percentile(&self, p: f64) -> f64 {
        percentile_of(self.results.iter().map(|r| r.prep_ms), p)
    }

    fn wait_percentile(&self, p: f64) -> f64 {
        percentile_of(self.results.iter().map(|r| r.wait_ms), p)
    }

    fn wait_max_ms(&self) -> f64 {
        self.results.iter().map(|r| r.wait_ms).fold(0.0, f64::max)
    }

    fn avg_latency_ms(&self) -> f64 {
        if self.results.is_empty() {
            0.0
        } else {
            self.results.iter().map(|r| r.latency_ms).sum::<f64>() / self.results.len() as f64
        }
    }

    fn min_latency_ms(&self) -> f64 {
        self.results
            .iter()
            .map(|r| r.latency_ms)
            .fold(f64::MAX, |a, b| a.min(b))
    }

    fn max_latency_ms(&self) -> f64 {
        self.results
            .iter()
            .map(|r| r.latency_ms)
            .fold(0.0, |a, b| a.max(b))
    }
}

fn percentile_of(values: impl Iterator<Item = f64>, p: f64) -> f64 {
    let mut values: Vec<f64> = values.collect();
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((values.len() as f64 - 1.0) * p / 100.0) as usize;
    values[idx.min(values.len() - 1)]
}

/// Records the operation breakdown (t0 = op start, t1 = request issued,
/// t2 = response decoded) for the per-op loops.
pub(crate) struct OpTiming {
    start: Instant,
    sent: Instant,
    end: Instant,
}

impl OpTiming {
    pub(crate) fn begin() -> Self {
        OP_SENT.with(|s| s.set(None));
        Self {
            start: instant_now(),
            sent: instant_now(),
            end: instant_now(),
        }
    }

    pub(crate) fn finish(mut self) -> Self {
        self.end = instant_now();
        self.sent = OP_SENT
            .with(|s| s.take())
            .filter(|sent| *sent >= self.start)
            .unwrap_or(self.start);
        self
    }

    fn latency_ms(&self) -> f64 {
        self.end.duration_since(self.start).as_secs_f64() * 1000.0
    }

    fn prep_ms(&self) -> f64 {
        self.sent.duration_since(self.start).as_secs_f64() * 1000.0
    }

    fn wait_ms(&self) -> f64 {
        self.end.duration_since(self.sent).as_secs_f64() * 1000.0
    }
}

pub(crate) fn op_result(timing: OpTiming, aborted: bool) -> OpResult {
    OpResult {
        latency_ms: timing.latency_ms(),
        prep_ms: timing.prep_ms(),
        wait_ms: timing.wait_ms(),
        aborted,
        sold_out: OP_SOLD_OUT.with(|c| c.replace(false)),
    }
}

pub(crate) fn print_summary(
    mode: &str,
    args: &Args,
    load_elapsed_secs: f64,
    txn_elapsed_secs: f64,
    total_elapsed_secs: f64,
    stats: &BenchmarkStats,
) {
    let throughput = if txn_elapsed_secs > 0.0 {
        stats.op_count() as f64 / txn_elapsed_secs
    } else {
        0.0
    };
    let tps = throughput;
    let committed_tps = if txn_elapsed_secs > 0.0 {
        (stats.op_count() - stats.abort_count()) as f64 / txn_elapsed_secs
    } else {
        0.0
    };
    let new_order_tps = tps * (args.new_order_percent as f64 / 100.0);
    let total_throughput = if total_elapsed_secs > 0.0 {
        stats.op_count() as f64 / total_elapsed_secs
    } else {
        0.0
    };
    let op_count = stats.op_count();
    let abort_count = stats.abort_count();
    let abort_rate = stats.abort_rate();
    let avg_latency = stats.avg_latency_ms();
    let min_latency = if op_count > 0 {
        stats.min_latency_ms()
    } else {
        0.0
    };
    let max_latency = if op_count > 0 {
        stats.max_latency_ms()
    } else {
        0.0
    };
    let p50 = stats.latency_percentile(50.0);
    let p90 = stats.latency_percentile(90.0);
    let p99 = stats.latency_percentile(99.0);
    let p999 = stats.latency_percentile(99.9);
    let prep_p50_us = stats.prep_percentile(50.0) * 1000.0;
    let prep_p99_us = stats.prep_percentile(99.0) * 1000.0;
    let wait_p50_us = stats.wait_percentile(50.0) * 1000.0;
    let wait_p99_us = stats.wait_percentile(99.0) * 1000.0;
    let wait_max_us = stats.wait_max_ms() * 1000.0;
    println!(
        "tpcc benchmark mode={mode} connections={} warehouses={} districts={} customers={} items={} operations={} load_elapsed={:.3}s txn_elapsed={:.3}s total_elapsed={:.3}s throughput={:.2} ops/s tps={:.2} committed_tps={:.2} new_order_tps={:.2} total_throughput={:.2} ops/s op_count={} abort_count={} abort_rate={:.2}% avg_latency={:.3}ms min_latency={:.3}ms max_latency={:.3}ms p50={:.3}ms p90={:.3}ms p99={:.3}ms p999={:.3}ms prep_p50_us={:.0} prep_p99_us={:.0} wait_p50_us={:.0} wait_p99_us={:.0} wait_max_us={:.0} sold_out={} think_time_ms={}",
        args.connection_count,
        args.warehouses,
        args.districts_per_warehouse,
        args.customers_per_district,
        args.items,
        args.operation_count,
        load_elapsed_secs,
        txn_elapsed_secs,
        total_elapsed_secs,
        throughput,
        tps,
        committed_tps,
        new_order_tps,
        total_throughput,
        op_count,
        abort_count,
        abort_rate,
        avg_latency,
        min_latency,
        max_latency,
        p50,
        p90,
        p99,
        p999,
        prep_p50_us,
        prep_p99_us,
        wait_p50_us,
        wait_p99_us,
        wait_max_us,
        stats.sold_out_count(),
        args.think_time_ms,
    );
}
