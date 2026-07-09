#!/usr/bin/env python3
"""
TPC-C benchmark orchestration script for MuduDB.

Supports:
- Single or multi-run benchmarks
- Backend comparison (iouring / tokio / legacy)
- Plotting results (TPS, latency percentiles, abort rate)

Usage:
    # Single run
    python3 script/bench/tpcc_benchmark.py --mode stored-procedure --warehouses 10 --operations 1000

    # Multi-run with averaging
    python3 script/bench/tpcc_benchmark.py --mode stored-procedure --warehouses 10 --operations 1000 --runs 3

    # Compare backends
    python3 script/bench/tpcc_benchmark.py --mode stored-procedure --warehouses 4 --operations 5000 --compare-backends

    # Plot results
    python3 script/bench/tpcc_benchmark.py --mode stored-procedure --warehouses 4 --operations 5000 --compare-backends --plot --output-dir ./bench_results
"""

import argparse
import json
import os
import re
import signal
import socket
import subprocess
import sys
import tempfile
import time
from contextlib import contextmanager
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Tuple


@dataclass
class TpccResult:
    mode: str
    warehouses: int
    districts: int
    customers: int
    items: int
    operations: int
    connections: int
    load_elapsed_sec: float
    txn_elapsed_sec: float
    total_elapsed_sec: float
    throughput: float
    tps: float
    new_order_tps: float
    total_throughput: float
    op_count: int
    abort_count: int
    abort_rate_pct: float
    avg_latency_ms: float
    min_latency_ms: float
    max_latency_ms: float
    p50_latency_ms: float
    p90_latency_ms: float
    p99_latency_ms: float
    p999_latency_ms: float
    server_mode: str = ""
    run_index: int = 0


def find_project_root() -> Path:
    path = Path(__file__).resolve()
    for parent in [path, *path.parents]:
        if (parent / "Cargo.toml").exists():
            return parent
    raise RuntimeError("Cannot find project root (no Cargo.toml in ancestry)")


def get_free_port(host: str = "127.0.0.1") -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind((host, 0))
        return s.getsockname()[1]


def wait_for_port(host: str, port: int, timeout: float = 30.0) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with socket.create_connection((host, port), timeout=1.0):
                return True
        except OSError:
            time.sleep(0.1)
    return False


def write_config(cfg_path: Path, args: argparse.Namespace, data_dir: Path) -> None:
    mpk_dir = data_dir / "mpk"
    mpk_dir.mkdir(parents=True, exist_ok=True)

    mode_map = {"legacy": 0, "iouring": 1, "tokio": 2}
    server_mode_num = mode_map.get(args.server_mode.lower(), 1)

    routing_map = {"connection_id": 0, "player_id": 1, "remote_hash": 2}
    routing_mode_num = routing_map.get(args.routing_mode.lower(), 0)

    config_text = f"""mpk_path = "{mpk_dir}"
data_path = "{data_dir}"
listen_ip = "{args.listen_ip}"
http_listen_port = {args.http_port}
pg_listen_port = 0
tcp_listen_port = {args.tcp_port}
server_mode = {server_mode_num}
tcp_multi_port = false
worker_threads = {args.worker_threads}
io_uring_ring_entries = {args.ring_entries}
io_uring_accept_multishot = true
io_uring_recv_multishot = true
io_uring_enable_fixed_buffers = false
io_uring_enable_fixed_files = false
routing_mode = {routing_mode_num}
enable_async = true
http_worker_threads = 1
"""
    cfg_path.write_text(config_text, encoding="utf-8")
    print(f"[config] wrote {cfg_path}")


def build_mpk(project_root: Path) -> Path:
    tpcc_dir = project_root / "example" / "tpcc"
    print(f"[build] building tpcc.mpk in {tpcc_dir} ...")
    result = subprocess.run(
        ["cargo", "make", "package"],
        cwd=tpcc_dir,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print(result.stdout)
        print(result.stderr, file=sys.stderr)
        raise RuntimeError("Failed to build tpcc.mpk")

    mpk_path = project_root / "target" / "wasm32-wasip2" / "release" / "tpcc.mpk"
    if not mpk_path.exists():
        raise RuntimeError(f"Expected mpk not found at {mpk_path}")
    print(f"[build] mpk ready: {mpk_path}")
    return mpk_path


def _find_mudud_binary(project_root: Path) -> Optional[Path]:
    for profile in ("release", "debug"):
        p = project_root / "target" / profile / "mudud"
        if p.exists():
            return p
    return None


def _mudud_supports_cfg_flag(mudud_bin: Path) -> bool:
    try:
        result = subprocess.run(
            [str(mudud_bin), "--help"],
            capture_output=True,
            text=True,
            timeout=3.0,
        )
        return result.returncode == 0 and "--cfg" in (result.stdout + result.stderr)
    except subprocess.TimeoutExpired:
        return False


@contextmanager
def managed_mudud(project_root: Path, cfg_path: Path, args: argparse.Namespace):
    mudud_bin = _find_mudud_binary(project_root)
    if mudud_bin is not None:
        if _mudud_supports_cfg_flag(mudud_bin):
            cmd = [str(mudud_bin), "serve", "--cfg", str(cfg_path)]
        else:
            cmd = [str(mudud_bin), "serve", str(cfg_path)]
    else:
        cmd = ["cargo", "run", "-p", "mudud", "--", "serve", "--cfg", str(cfg_path)]
    print(f"[server] starting: {' '.join(cmd)}")
    log_path = cfg_path.parent / "mudud.log"
    log_file = open(log_path, "w")
    proc = subprocess.Popen(
        cmd,
        cwd=project_root,
        stdout=log_file,
        stderr=subprocess.STDOUT,
        text=True,
    )

    try:
        http_ready = wait_for_port(args.listen_ip, args.http_port, timeout=args.server_start_timeout)
        tcp_ready = wait_for_port(args.listen_ip, args.tcp_port, timeout=args.server_start_timeout)
        if not http_ready or not tcp_ready:
            log_file.flush()
            log_tail = log_path.read_text(encoding="utf-8", errors="replace")[-2000:]
            if log_tail:
                print(log_tail, file=sys.stderr)
            raise RuntimeError(
                f"Server failed to bind ports within {args.server_start_timeout}s "
                f"(http_ready={http_ready}, tcp_ready={tcp_ready})"
            )
        print(f"[server] ready on http={args.http_port} tcp={args.tcp_port}")
        time.sleep(1.0)
        yield proc
    finally:
        print("[server] stopping ...")
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=10.0)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        log_file.close()
        print("[server] stopped")


def run_benchmark(
    project_root: Path,
    args: argparse.Namespace,
    mpk_path: Optional[Path],
) -> TpccResult:
    bench_args = [
        "cargo", "run", "-p", "tpcc",
        "--features", "benchmark-runner",
        "--bin", "tpcc-benchmark", "--",
        "--mode", args.mode,
        "--warehouses", str(args.warehouses),
        "--districts-per-warehouse", str(args.districts),
        "--customers-per-district", str(args.customers),
        "--items", str(args.items),
        "--operation-count", str(args.operations),
        "--connection-count", str(args.connections),
        "--payment-percent", str(args.payment_percent),
        "--new-order-percent", str(args.new_order_percent),
        "--tcp-addr", f"{args.listen_ip}:{args.tcp_port}",
        "--http-addr", f"{args.listen_ip}:{args.http_port}",
    ]

    if args.warehouse_partitioned:
        bench_args.append("--warehouse-partitioned")
    if args.mode == "stored-procedure" and mpk_path is not None:
        bench_args.extend(["--mpk", str(mpk_path)])

    print(f"[bench] running: {' '.join(bench_args)}")
    result = subprocess.run(
        bench_args,
        cwd=project_root,
        capture_output=True,
        text=True,
    )

    print(result.stdout)
    if result.stderr:
        print(result.stderr, file=sys.stderr)

    if result.returncode != 0:
        raise RuntimeError(f"Benchmark exited with code {result.returncode}")

    return parse_benchmark_output(result.stdout, args)


def parse_benchmark_output(stdout: str, args: argparse.Namespace) -> TpccResult:
    pattern = re.compile(
        r"tpcc benchmark mode=(\S+) "
        r"connections=(\d+) warehouses=(\d+) districts=(\d+) customers=(\d+) items=(\d+) "
        r"operations=(\d+) load_elapsed=([\d.]+)s txn_elapsed=([\d.]+)s total_elapsed=([\d.]+)s "
        r"throughput=([\d.]+) ops/s tps=([\d.]+) new_order_tps=([\d.]+) total_throughput=([\d.]+) ops/s "
        r"op_count=(\d+) abort_count=(\d+) abort_rate=([\d.]+)% "
        r"avg_latency=([\d.]+)ms min_latency=([\d.]+)ms max_latency=([\d.]+)ms "
        r"p50=([\d.]+)ms p90=([\d.]+)ms p99=([\d.]+)ms p999=([\d.]+)ms"
    )

    for line in stdout.splitlines():
        m = pattern.search(line)
        if m:
            return TpccResult(
                mode=m.group(1),
                warehouses=int(m.group(3)),
                districts=int(m.group(4)),
                customers=int(m.group(5)),
                items=int(m.group(6)),
                operations=int(m.group(7)),
                connections=int(m.group(2)),
                load_elapsed_sec=float(m.group(8)),
                txn_elapsed_sec=float(m.group(9)),
                total_elapsed_sec=float(m.group(10)),
                throughput=float(m.group(11)),
                tps=float(m.group(12)),
                new_order_tps=float(m.group(13)),
                total_throughput=float(m.group(14)),
                op_count=int(m.group(15)),
                abort_count=int(m.group(16)),
                abort_rate_pct=float(m.group(17)),
                avg_latency_ms=float(m.group(18)),
                min_latency_ms=float(m.group(19)),
                max_latency_ms=float(m.group(20)),
                p50_latency_ms=float(m.group(21)),
                p90_latency_ms=float(m.group(22)),
                p99_latency_ms=float(m.group(23)),
                p999_latency_ms=float(m.group(24)),
                server_mode=getattr(args, "server_mode", ""),
            )

    raise RuntimeError("Could not parse benchmark summary from output")


def print_result(result: TpccResult, fmt: str) -> None:
    if fmt == "json":
        print(json.dumps(asdict(result), indent=2))
    elif fmt == "jsonl":
        print(json.dumps(asdict(result)))
    else:
        print("\n" + "=" * 60)
        print("TPC-C Benchmark Result")
        print("=" * 60)
        print(f"  Mode:              {result.mode}")
        print(f"  Server mode:       {result.server_mode}")
        print(f"  Warehouses:        {result.warehouses}")
        print(f"  Districts:         {result.districts}")
        print(f"  Customers:         {result.customers}")
        print(f"  Items:             {result.items}")
        print(f"  Operations:        {result.operations}")
        print(f"  Connections:       {result.connections}")
        print(f"  Load elapsed:      {result.load_elapsed_sec:.3f} s")
        print(f"  Txn elapsed:       {result.txn_elapsed_sec:.3f} s")
        print(f"  Total elapsed:     {result.total_elapsed_sec:.3f} s")
        print(f"  Throughput:        {result.throughput:.2f} ops/s")
        print(f"  TPS:               {result.tps:.2f}")
        print(f"  New-Order TPS:     {result.new_order_tps:.2f}")
        print(f"  Total throughput:  {result.total_throughput:.2f} ops/s")
        print(f"  Op count:          {result.op_count}")
        print(f"  Abort count:       {result.abort_count}")
        print(f"  Abort rate:        {result.abort_rate_pct:.2f}%")
        print(f"  Avg latency:       {result.avg_latency_ms:.3f} ms")
        print(f"  Min latency:       {result.min_latency_ms:.3f} ms")
        print(f"  Max latency:       {result.max_latency_ms:.3f} ms")
        print(f"  P50 latency:       {result.p50_latency_ms:.3f} ms")
        print(f"  P90 latency:       {result.p90_latency_ms:.3f} ms")
        print(f"  P99 latency:       {result.p99_latency_ms:.3f} ms")
        print(f"  P999 latency:      {result.p999_latency_ms:.3f} ms")
        print("=" * 60)


def run_single_benchmark(
    project_root: Path,
    args: argparse.Namespace,
    mpk_path: Optional[Path],
    data_dir: Path,
    run_index: int = 0,
) -> TpccResult:
    """Run one benchmark iteration with a fresh data directory."""
    (data_dir / "mpk").mkdir(parents=True, exist_ok=True)

    if args.mode == "stored-procedure":
        if args.http_port == 8300 and args.tcp_port == 9527:
            args.http_port = get_free_port(args.listen_ip)
            args.tcp_port = get_free_port(args.listen_ip)

    cfg_path = data_dir / "mudud.cfg"
    write_config(cfg_path, args, data_dir)

    try:
        if args.mode == "stored-procedure":
            with managed_mudud(project_root, cfg_path, args) as _proc:
                result = run_benchmark(project_root, args, mpk_path)
        else:
            result = run_benchmark(project_root, args, mpk_path)
        result.server_mode = args.server_mode
        result.run_index = run_index
        return result
    finally:
        import shutil
        shutil.rmtree(data_dir, ignore_errors=True)


def aggregate_results(results: List[TpccResult]) -> TpccResult:
    """Compute mean values across multiple runs."""
    if not results:
        raise ValueError("No results to aggregate")
    n = len(results)
    base = results[0]

    def avg(field_name: str) -> float:
        return sum(getattr(r, field_name) for r in results) / n

    return TpccResult(
        mode=base.mode,
        warehouses=base.warehouses,
        districts=base.districts,
        customers=base.customers,
        items=base.items,
        operations=base.operations,
        connections=base.connections,
        load_elapsed_sec=avg("load_elapsed_sec"),
        txn_elapsed_sec=avg("txn_elapsed_sec"),
        total_elapsed_sec=avg("total_elapsed_sec"),
        throughput=avg("throughput"),
        tps=avg("tps"),
        new_order_tps=avg("new_order_tps"),
        total_throughput=avg("total_throughput"),
        op_count=base.op_count,
        abort_count=int(avg("abort_count")),
        abort_rate_pct=avg("abort_rate_pct"),
        avg_latency_ms=avg("avg_latency_ms"),
        min_latency_ms=min(r.min_latency_ms for r in results),
        max_latency_ms=max(r.max_latency_ms for r in results),
        p50_latency_ms=avg("p50_latency_ms"),
        p90_latency_ms=avg("p90_latency_ms"),
        p99_latency_ms=avg("p99_latency_ms"),
        p999_latency_ms=avg("p999_latency_ms"),
        server_mode=base.server_mode,
    )


def print_comparison_table(results_by_backend: Dict[str, List[TpccResult]]) -> None:
    """Print a comparison table across backends."""
    print("\n" + "=" * 100)
    print("Backend Comparison")
    print("=" * 100)
    header = (
        f"{'Backend':<12} {'Runs':>4} {'TPS':>10} {'Throughput':>12} "
        f"{'Abort%':>8} {'P50(ms)':>10} {'P90(ms)':>10} {'P99(ms)':>10} {'P999(ms)':>10}"
    )
    print(header)
    print("-" * 100)

    for backend, results in results_by_backend.items():
        if not results:
            print(f"{backend:<12} {0:>4} {'N/A':>10} {'N/A':>12} {'N/A':>8} {'N/A':>10} {'N/A':>10} {'N/A':>10} {'N/A':>10}")
            continue
        agg = aggregate_results(results)
        print(
            f"{backend:<12} {len(results):>4} {agg.tps:>10.2f} {agg.throughput:>12.2f} "
            f"{agg.abort_rate_pct:>8.2f} {agg.p50_latency_ms:>10.3f} {agg.p90_latency_ms:>10.3f} "
            f"{agg.p99_latency_ms:>10.3f} {agg.p999_latency_ms:>10.3f}"
        )
    print("=" * 100)


def plot_results(
    results_by_backend: Dict[str, List[TpccResult]],
    output_dir: Path,
    args: argparse.Namespace,
) -> None:
    """Generate comparison charts."""
    try:
        import matplotlib
        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError as e:
        print(f"[warn] matplotlib not available, skipping plots: {e}")
        return

    output_dir.mkdir(parents=True, exist_ok=True)
    backends = list(results_by_backend.keys())
    aggregated = {b: aggregate_results(results_by_backend[b]) for b in backends if results_by_backend[b]}

    # ---- Chart 1: TPS bar chart ----
    fig, ax = plt.subplots(figsize=(8, 5))
    tps_vals = [aggregated[b].tps for b in backends]
    colors = ["#2ecc71", "#3498db", "#e74c3c"]
    bars = ax.bar(backends, tps_vals, color=colors[: len(backends)], edgecolor="black")
    ax.set_ylabel("TPS")
    ax.set_title(f"TPC-C TPS Comparison\n({args.warehouses} warehouses, {args.operations} ops, {args.connections} conn)")
    for bar in bars:
        height = bar.get_height()
        ax.annotate(
            f"{height:.2f}",
            xy=(bar.get_x() + bar.get_width() / 2, height),
            xytext=(0, 3),
            textcoords="offset points",
            ha="center",
            va="bottom",
            fontsize=10,
        )
    plt.tight_layout()
    plt.savefig(output_dir / "tps_comparison.png", dpi=150)
    plt.close()
    print(f"[plot] saved {output_dir / 'tps_comparison.png'}")

    # ---- Chart 2: Latency percentiles grouped bar ----
    fig, ax = plt.subplots(figsize=(10, 6))
    x = range(len(backends))
    width = 0.2
    metrics = ["p50_latency_ms", "p90_latency_ms", "p99_latency_ms", "p999_latency_ms"]
    labels = ["P50", "P90", "P99", "P999"]
    for i, (metric, label) in enumerate(zip(metrics, labels)):
        vals = [getattr(aggregated[b], metric) for b in backends]
        ax.bar([xi + width * i for xi in x], vals, width, label=label)

    ax.set_ylabel("Latency (ms)")
    ax.set_xticks([xi + width * 1.5 for xi in x])
    ax.set_xticklabels(backends)
    ax.set_title(f"Latency Percentile Comparison\n({args.warehouses} warehouses, {args.operations} ops)")
    ax.legend()
    plt.tight_layout()
    plt.savefig(output_dir / "latency_comparison.png", dpi=150)
    plt.close()
    print(f"[plot] saved {output_dir / 'latency_comparison.png'}")

    # ---- Chart 3: Abort rate bar chart ----
    fig, ax = plt.subplots(figsize=(8, 5))
    abort_vals = [aggregated[b].abort_rate_pct for b in backends]
    bars = ax.bar(backends, abort_vals, color=colors[: len(backends)], edgecolor="black")
    ax.set_ylabel("Abort Rate (%)")
    ax.set_title(f"Abort Rate Comparison\n({args.warehouses} warehouses, {args.operations} ops)")
    for bar in bars:
        height = bar.get_height()
        ax.annotate(
            f"{height:.2f}%",
            xy=(bar.get_x() + bar.get_width() / 2, height),
            xytext=(0, 3),
            textcoords="offset points",
            ha="center",
            va="bottom",
            fontsize=10,
        )
    plt.tight_layout()
    plt.savefig(output_dir / "abort_rate_comparison.png", dpi=150)
    plt.close()
    print(f"[plot] saved {output_dir / 'abort_rate_comparison.png'}")

    # ---- Chart 4: Multi-run trend (if runs > 1) ----
    max_runs = max(len(v) for v in results_by_backend.values())
    if max_runs > 1:
        fig, ax = plt.subplots(figsize=(10, 6))
        for backend, results in results_by_backend.items():
            tps_series = [r.tps for r in results]
            ax.plot(range(1, len(tps_series) + 1), tps_series, marker="o", label=backend)
        ax.set_xlabel("Run")
        ax.set_ylabel("TPS")
        ax.set_title("TPS Trend Across Runs")
        ax.legend()
        ax.grid(True, linestyle="--", alpha=0.5)
        plt.tight_layout()
        plt.savefig(output_dir / "tps_trend.png", dpi=150)
        plt.close()
        print(f"[plot] saved {output_dir / 'tps_trend.png'}")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run TPC-C benchmark against MuduDB",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )

    # Benchmark parameters
    parser.add_argument("--mode", choices=["interactive", "stored-procedure"], default="interactive")
    parser.add_argument("--warehouses", type=int, default=10)
    parser.add_argument("--districts", type=int, default=10)
    parser.add_argument("--customers", type=int, default=100)
    parser.add_argument("--items", type=int, default=100)
    parser.add_argument("--operations", type=int, default=1000)
    parser.add_argument("--connections", type=int, default=4)
    parser.add_argument("--payment-percent", type=int, default=50)
    parser.add_argument("--new-order-percent", type=int, default=35)
    parser.add_argument("--warehouse-partitioned", action="store_true")

    # Server parameters
    parser.add_argument("--server-mode", choices=["legacy", "iouring", "tokio"], default="iouring")
    parser.add_argument("--worker-threads", type=int, default=0, help="0 = auto-detect CPU cores")
    parser.add_argument("--ring-entries", type=int, default=1024)
    parser.add_argument("--routing-mode", choices=["connection_id", "player_id", "remote_hash"], default="connection_id")

    # Network parameters
    parser.add_argument("--listen-ip", default="127.0.0.1")
    parser.add_argument("--http-port", type=int, default=8300)
    parser.add_argument("--tcp-port", type=int, default=9527)

    # Build / mpk parameters
    parser.add_argument("--build-mpk", action="store_true", help="Build tpcc.mpk before running")
    parser.add_argument("--mpk-path", type=Path, default=None, help="Path to existing tpcc.mpk")

    # Multi-run / comparison / plotting
    parser.add_argument("--runs", type=int, default=1, help="Number of runs per configuration")
    parser.add_argument("--compare-backends", action="store_true", help="Compare legacy/iouring/tokio backends")
    parser.add_argument("--plot", action="store_true", help="Generate comparison charts")
    parser.add_argument("--output-dir", type=Path, default=Path("./bench_results"), help="Directory for plots")
    parser.add_argument("--server-start-timeout", type=float, default=60.0)
    parser.add_argument("--output-format", choices=["table", "json", "jsonl"], default="table")

    args = parser.parse_args()
    project_root = find_project_root()
    print(f"[info] project root: {project_root}")

    # Build mpk once if needed
    mpk_path: Optional[Path] = None
    if args.mode == "stored-procedure":
        if args.mpk_path is not None:
            mpk_path = args.mpk_path.resolve()
        elif args.build_mpk:
            mpk_path = build_mpk(project_root)
        else:
            default_mpk = project_root / "target" / "wasm32-wasip2" / "release" / "tpcc.mpk"
            if default_mpk.exists():
                mpk_path = default_mpk
                print(f"[info] using existing mpk: {mpk_path}")
            else:
                print("[warn] no mpk found; use --build-mpk or --mpk-path")

    backends_to_test: List[str]
    if args.compare_backends and args.mode == "stored-procedure":
        # Legacy backend does not expose a TCP listener for stored procedures
        backends_to_test = ["iouring", "tokio"]
        print("[info] legacy backend excluded: it does not support stored-procedure TCP mode")
    elif args.compare_backends and args.mode == "interactive":
        backends_to_test = ["iouring", "tokio", "legacy"]
    else:
        backends_to_test = [args.server_mode]

    results_by_backend: Dict[str, List[TpccResult]] = {}

    for backend in backends_to_test:
        print(f"\n{'='*60}")
        print(f"Backend: {backend}")
        print(f"{'='*60}")
        backend_results: List[TpccResult] = []

        for run in range(args.runs):
            if args.runs > 1:
                print(f"\n--- Run {run + 1}/{args.runs} ---")

            data_dir = Path(tempfile.mkdtemp(prefix="tpcc_bench_"))
            run_args = argparse.Namespace(**vars(args))
            run_args.server_mode = backend

            try:
                result = run_single_benchmark(project_root, run_args, mpk_path, data_dir, run_index=run)
                backend_results.append(result)
                if not args.compare_backends and args.runs == 1:
                    print_result(result, args.output_format)
            except Exception as e:
                print(f"[error] run {run + 1} failed: {e}", file=sys.stderr)
                import shutil
                shutil.rmtree(data_dir, ignore_errors=True)
                if args.runs == 1:
                    return 1

        results_by_backend[backend] = backend_results

    # Print comparison if multiple backends or multiple runs
    if args.compare_backends or args.runs > 1:
        print_comparison_table(results_by_backend)

    # Plot if requested
    if args.plot and len(results_by_backend) > 0:
        plot_results(results_by_backend, args.output_dir, args)

    # Save JSON summary
    summary = {
        "config": {
            "mode": args.mode,
            "warehouses": args.warehouses,
            "operations": args.operations,
            "connections": args.connections,
            "runs": args.runs,
        },
        "results": {
            backend: [asdict(r) for r in results]
            for backend, results in results_by_backend.items()
        },
    }
    summary_path = args.output_dir / "benchmark_summary.json"
    summary_path.parent.mkdir(parents=True, exist_ok=True)
    summary_path.write_text(json.dumps(summary, indent=2))
    print(f"[summary] saved {summary_path}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
