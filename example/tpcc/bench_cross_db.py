#!/usr/bin/env python3
"""TPC-C cross-database benchmark orchestrator.

Compares TPC-C throughput (TPS) and P99 latency across these configurations:

1. PostgreSQL via mudu_adapter (interactive mode)
2. PostgreSQL stored-procedure mode (PL/pgSQL functions invoked by the
   tpcc-benchmark client with --mode pg-procedure; label `postgres-n`)
3. MySQL via mudu_adapter (interactive mode)
4. MuduDB port-sharding modes, one per entry in the config's `mudud.modes`
   mapping; each entry combines a server mode (tokio/iouring) with an
   interactive mode (interactive/procedure), e.g. interactive (tokio),
   procedure (tokio), procedure-iouring (iouring)
5. SpacetimeDB reducer mode via the tpcc-stdb-benchmark client (opt-in:
   not part of the default --backends list; pass --backends spacetimedb
   to include it)

Parameter sweeps:
- CPU core limits (physical cores only, using Linux taskset)
- Connection counts (from under-loaded to over-loaded)

Outputs:
- Structured JSON raw results
- Aggregated CSV summary
- Matplotlib charts (TPS/P99 vs connections/cores)
- Companion Markdown documentation

Usage:
    cd example/tpcc
    python3 bench_cross_db.py --config bench_cross_db_config.yaml
"""

import argparse
import csv
import fnmatch
import getpass
import json
import os
import re
import resource
import shlex
import shutil
import signal
import socket
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
from abc import ABC, abstractmethod
from contextlib import contextmanager
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Dict, Iterable, List, NamedTuple, Optional, Tuple


# ---------------------------------------------------------------------------
# Configuration helpers
# ---------------------------------------------------------------------------


def _load_yaml(path: Path) -> Any:
    import yaml

    return yaml.safe_load(path.read_text(encoding="utf-8"))


def _load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def load_config(path: Path) -> Dict[str, Any]:
    """Load a YAML or JSON configuration file."""
    suffix = path.suffix.lower()
    if suffix in (".yaml", ".yml"):
        return _load_yaml(path)
    if suffix == ".json":
        return _load_json(path)
    # Try YAML first, then JSON.
    try:
        return _load_yaml(path)
    except Exception as e_yaml:
        try:
            return _load_json(path)
        except Exception as e_json:
            raise RuntimeError(
                f"Could not parse {path} as YAML or JSON: {e_yaml}; {e_json}"
            )


# Keys a test entry may override from `defaults` (sweep and workload
# parameters only). Infrastructure keys (backend binaries/lifecycle, remote,
# output/work dirs, client pinning, buffer pool) are global and must stay in
# `defaults`.
TEST_OVERRIDABLE_KEYS = {
    "backends",
    "workload",
    "cpu_cores",
    "connections",
    "seckill_hotspot_percents",
    "seckill_items",
    "seckill_payload_bytes",
    "hot_rows_per_warehouse",
    "order_lines",
    "repeats",
    "warmup_operations",
    "think_times_ms",
    "operations",
    "warehouses",
    "districts",
    "customers",
    "items",
    "payment_percent",
    "new_order_percent",
    "benchmark_timeout_secs",
    "perf_sample_rate",
}


def expand_config_tests(cfg_dict: Dict[str, Any]) -> List[Tuple[str, Dict[str, Any]]]:
    """Expand a config into (test name, merged config dict) pairs.

    The config must be a mapping with `defaults` (a mapping, may be empty)
    and `tests` (a non-empty list of mappings, each with a unique string
    `name`). Each test's settings merge shallowly over the defaults; a test
    may only override the keys in TEST_OVERRIDABLE_KEYS. The old flat
    (single-sweep) config format is rejected.
    """
    if not isinstance(cfg_dict, dict):
        raise ValueError(
            "config must be a mapping with 'defaults' and 'tests' sections, "
            f"got {type(cfg_dict).__name__}"
        )
    unknown_top = sorted(k for k in cfg_dict if k not in ("defaults", "tests"))
    if unknown_top:
        raise ValueError(
            f"config has unknown top-level keys {unknown_top}; the config "
            "format is now {defaults: {...}, tests: [{name: ...}, ...]} — "
            "move global settings under 'defaults' and per-sweep overrides "
            "into named 'tests' entries"
        )
    defaults = cfg_dict.get("defaults") or {}
    if not isinstance(defaults, dict):
        raise ValueError(
            f"config 'defaults' must be a mapping, got {type(defaults).__name__}"
        )
    tests_raw = cfg_dict.get("tests")
    if not isinstance(tests_raw, list) or not tests_raw:
        raise ValueError(
            "config 'tests' must be a non-empty list of named test mappings; "
            "the old flat config format is no longer supported"
        )
    tests: List[Tuple[str, Dict[str, Any]]] = []
    seen_names = set()
    for entry in tests_raw:
        if not isinstance(entry, dict):
            raise ValueError(
                f"each test entry must be a mapping, got {type(entry).__name__}"
            )
        name = entry.get("name")
        if not isinstance(name, str) or not name:
            raise ValueError(
                f"each test entry needs a non-empty string 'name', got {name!r}"
            )
        if name in seen_names:
            raise ValueError(f"duplicate test name '{name}'")
        seen_names.add(name)
        overrides = {k: v for k, v in entry.items() if k != "name"}
        bad = sorted(k for k in overrides if k not in TEST_OVERRIDABLE_KEYS)
        if bad:
            raise ValueError(
                f"test '{name}' sets keys that are not test-overridable: "
                f"{bad}; tests may only override "
                f"{sorted(TEST_OVERRIDABLE_KEYS)}"
            )
        tests.append((name, {**defaults, **overrides}))
    return tests


# ---------------------------------------------------------------------------
# Dataclasses
# ---------------------------------------------------------------------------


@dataclass
class BenchConfig:
    """Top-level benchmark configuration."""

    # Sweep parameters
    cpu_cores: List[int]
    connections: List[int]
    repeats: int
    warmup_operations: int

    # TPC-C dataset
    warehouses: int
    districts: int
    customers: int
    items: int
    operations: int
    payment_percent: int
    new_order_percent: int
    # DB binaries / lifecycle
    postgres: Dict[str, Any]
    mysql: Dict[str, Any]
    mudud: Dict[str, Any]
    spacetimedb: Dict[str, Any]

    # Affinity
    client_cpu_cores: Optional[List[int]]
    client_cpu_offset: int

    # Output
    output_dir: Path
    keep_data: bool
    work_dir: Optional[Path] = None
    # Fraction of system RAM given to the DB buffer pool (PostgreSQL
    # shared_buffers / MySQL innodb_buffer_pool_size). MuduDB has no buffer
    # pool configuration and is unaffected.
    buffer_pool_ratio: float = 0.8
    # End-to-end perf sampling rate for the tpcc benchmark client
    # (0 = disabled, N = sample 1/N requests, breakdown goes to the client log).
    perf_sample_rate: int = 0
    # Workload driver: "tpcc" (default, original TPC-C mix) or "seckill"
    # (write-heavy flash-sale mix; both benchmark clients accept --workload).
    workload: str = "tpcc"
    # Flash-sale item count / order payload bytes (only used when
    # workload = "seckill").
    seckill_items: int = 320
    seckill_payload_bytes: int = 2048
    # Percentages of flash-sale operations (0-100) routed to one hot item,
    # swept per run (0 = uniform over all items; only used when
    # workload = "seckill").
    seckill_hotspot_percents: List[int] = field(default_factory=lambda: [0])
    # Hot-row contention injector: hotspot rows per warehouse (0 = off).
    hot_rows_per_warehouse: int = 0
    # Fixed order-line count per new-order (0 = original 3-7 variable mix).
    order_lines: int = 0
    # Per-terminal think time in milliseconds slept between transactions
    # (0 = off); paces the offered load, excluded from per-op latency. Both
    # benchmark clients accept --think-time-ms. Sweep dimension: each value
    # gets its own run tier. The default is the TPC-C mix-weighted mean think
    # time (~11s): the spec (Clause 5.2.5.4) defines per-type means of 12s
    # (New-Order, Payment), 10s (Order-Status), and 5s (Delivery,
    # Stock-Level).
    think_times_ms: List[int] = field(default_factory=lambda: [11000])
    # Per-run wall-clock limit for the benchmark client in seconds. The full
    # scale seed inserts millions of rows, so this must comfortably exceed the
    # seed time plus the measured phase.
    benchmark_timeout_secs: int = 3600
    # Two-machine (remote) mode settings; empty dict = local mode (default).
    # See RemoteConfig for the supported keys.
    remote: Dict[str, Any] = field(default_factory=dict)

    @staticmethod
    def from_dict(cfg: Dict[str, Any]) -> "BenchConfig":
        return BenchConfig(
            cpu_cores=cfg.get("cpu_cores", [1, 2, 4]),
            connections=cfg.get("connections", [1, 2, 4, 8, 16]),
            repeats=cfg.get("repeats", 3),
            warmup_operations=cfg.get("warmup_operations", 0),
            warehouses=cfg.get("warehouses", 4),
            districts=cfg.get("districts", 10),
            customers=cfg.get("customers", 100),
            items=cfg.get("items", 100),
            operations=cfg.get("operations", 1000),
            payment_percent=cfg.get("payment_percent", 50),
            new_order_percent=cfg.get("new_order_percent", 35),
            postgres=cfg.get("postgres", {}),
            mysql=cfg.get("mysql", {}),
            mudud=cfg.get("mudud", {}),
            spacetimedb=cfg.get("spacetimedb", {}),
            client_cpu_cores=cfg.get("client_cpu_cores"),
            client_cpu_offset=cfg.get("client_cpu_offset", 0),
            output_dir=Path(cfg.get("output_dir", "./bench_cross_db_results")),
            keep_data=cfg.get("keep_data", False),
            work_dir=Path(cfg["work_dir"]) if cfg.get("work_dir") else None,
            buffer_pool_ratio=float(cfg.get("buffer_pool_ratio", 0.8)),
            perf_sample_rate=int(cfg.get("perf_sample_rate", 0)),
            workload=str(cfg.get("workload", "tpcc")),
            seckill_items=int(cfg.get("seckill_items", 320)),
            seckill_payload_bytes=int(cfg.get("seckill_payload_bytes", 2048)),
            seckill_hotspot_percents=[
                int(v) for v in cfg.get("seckill_hotspot_percents", [0])
            ],
            hot_rows_per_warehouse=int(cfg.get("hot_rows_per_warehouse", 0)),
            order_lines=int(cfg.get("order_lines", 0)),
            think_times_ms=[int(v) for v in cfg.get("think_times_ms", [11000])],
            benchmark_timeout_secs=int(cfg.get("benchmark_timeout_secs", 3600)),
            remote=dict(cfg.get("remote", {}) or {}),
        )


class RemoteConfig:
    """Settings for two-machine (remote) mode.

    The client machine runs the benchmark scan and the benchmark clients;
    the database servers are started and stopped on the server machine over
    SSH (see ParamikoSsh / RemoteBackend). Remote mode is enabled iff
    `server_host` is set in the config's `remote` section.

    Fixed ports are used in remote mode (the client must know them in
    advance to wait for readiness and to connect); `mudud_tcp_port` is the
    base of a block of `cores` consecutive ports (tcp_multi_port).
    """

    def __init__(self, raw: Dict[str, Any]) -> None:
        self.server_host = str(raw.get("server_host", "") or "")
        self.ssh_user = str(raw.get("ssh_user", "") or "") or getpass.getuser()
        self.ssh_port = int(raw.get("ssh_port", 22))
        self.ssh_key_filename = str(raw.get("ssh_key_filename", "") or "")
        self.ssh_password = str(raw.get("ssh_password", "") or "")
        self.server_project_root = str(raw.get("server_project_root", "") or "")
        self.postgres_port = int(raw.get("postgres_port", 55432))
        self.mysql_port = int(raw.get("mysql_port", 53306))
        self.mudud_tcp_port = int(raw.get("mudud_tcp_port", 54000))
        self.mudud_http_port = int(raw.get("mudud_http_port", 58080))
        self.spacetimedb_port = int(raw.get("spacetimedb_port", 53000))

    @property
    def enabled(self) -> bool:
        return bool(self.server_host)


def remote_enabled(cfg: BenchConfig) -> bool:
    """True when the config has a remote section with server_host set."""
    return bool(cfg.remote.get("server_host"))


@dataclass
class TpccResult:
    backend: str
    mode: str
    cores: int
    connections: int
    run: int
    warehouses: int
    districts: int
    customers: int
    items: int
    operations: int
    load_elapsed_sec: float
    txn_elapsed_sec: float
    total_elapsed_sec: float
    throughput: float
    tps: float
    committed_tps: float
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
    error: str = ""
    # Which server binary produced this result (e.g. "target/release/mudud
    # (release)"); empty for backends without a local server binary.
    server_binary: str = ""
    # Per-run seckill hotspot percentage this result was measured with
    # (0 = uniform; only meaningful when workload = "seckill").
    seckill_hotspot_percent: int = 0
    # Per-run per-terminal think time in milliseconds (0 = off).
    think_time_ms: int = 0


# ---------------------------------------------------------------------------
# Utility functions
# ---------------------------------------------------------------------------


def find_project_root() -> Path:
    """Find the workspace root by looking for a Cargo.toml containing [workspace]."""
    path = Path(__file__).resolve().parent
    for parent in [path, *path.parents]:
        cargo_toml = parent / "Cargo.toml"
        if cargo_toml.exists() and "[workspace]" in cargo_toml.read_text(encoding="utf-8"):
            return parent
    raise RuntimeError("Cannot find workspace root (no [workspace] Cargo.toml in ancestry)")


def get_free_port(host: str = "127.0.0.1") -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind((host, 0))
        return s.getsockname()[1]


def get_free_port_block(count: int, host: str = "127.0.0.1") -> int:
    """Find a base port such that base .. base+count-1 are all bindable.

    The mududb server with tcp_multi_port=true binds one listener per worker
    at consecutive ports (base + worker_index), so the whole block must be
    free — checking only the base port lets a collision on a later port
    through, which surfaces much later as "server is not ready".
    """
    if count <= 0:
        raise ValueError(f"port block size must be positive, got {count}")
    if count == 1:
        return get_free_port(host)
    for _ in range(128):
        base = get_free_port(host)
        if base + count > 65536:
            continue
        sockets = []
        try:
            for offset in range(count):
                s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                s.bind((host, base + offset))
                sockets.append(s)
        except OSError:
            for s in sockets:
                s.close()
            continue
        for s in sockets:
            s.close()
        return base
    raise RuntimeError(f"no free block of {count} consecutive TCP ports found")


def wait_for_port(host: str, port: int, timeout: float = 60.0) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with socket.create_connection((host, port), timeout=1.0):
                return True
        except OSError:
            time.sleep(0.1)
    return False


def wait_for_file(path: Path, timeout: float = 60.0) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if path.exists():
            return True
        time.sleep(0.1)
    return False


def run_cmd(
    cmd: List[str],
    cwd: Optional[Path] = None,
    env: Optional[Dict[str, str]] = None,
    timeout: Optional[float] = None,
    check: bool = True,
    capture: bool = True,
) -> subprocess.CompletedProcess:
    """Run a shell command and return the completed process."""
    kwargs: Dict[str, Any] = {}
    if cwd is not None:
        kwargs["cwd"] = str(cwd)
    if env is not None:
        kwargs["env"] = env
    if timeout is not None:
        kwargs["timeout"] = timeout
    if capture:
        kwargs["capture_output"] = True
        kwargs["text"] = True
    result = subprocess.run(cmd, **kwargs)
    if check and result.returncode != 0:
        stdout = result.stdout if capture else "<not captured>"
        stderr = result.stderr if capture else "<not captured>"
        raise RuntimeError(
            f"Command failed: {' '.join(cmd)}\n"
            f"exit={result.returncode}\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}"
        )
    return result


def _physical_cores_from_lscpu(text: str) -> Optional[int]:
    """Parse `lscpu -p=CPU,Core[,...]` output into a physical core count.

    The number of distinct core ids is a good proxy for physical cores.
    Returns None when the output is unusable.
    """
    cpus = []
    threads_per_core: Dict[int, int] = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split(",")
        if len(parts) >= 2:
            cpu = int(parts[0])
            core = int(parts[1])
            cpus.append((cpu, core))
            threads_per_core[core] = threads_per_core.get(core, 0) + 1
    if cpus and threads_per_core:
        return len(threads_per_core)
    return None


def detect_physical_cores() -> int:
    """Detect the number of physical CPU cores.

    Prefers lscpu. Falls back to os.cpu_count() / threads_per_core heuristic.
    """
    try:
        proc = run_cmd(["lscpu", "-p=CPU,Core,Socket,MAXMHZ"], capture=True, check=True)
        cores = _physical_cores_from_lscpu(proc.stdout)
        if cores is not None:
            return cores
    except Exception:
        pass

    try:
        proc = run_cmd(["nproc"], capture=True, check=True)
        total = int(proc.stdout.strip())
    except Exception:
        total = os.cpu_count() or 1

    try:
        proc = run_cmd(["lscpu"], capture=True, check=True)
        tpc = 1
        for line in proc.stdout.splitlines():
            if "Thread(s) per core:" in line:
                tpc = int(line.split(":")[1].strip())
                break
        return max(1, total // tpc)
    except Exception:
        return total


def build_core_mask(count: int, offset: int = 0, available: Optional[int] = None) -> str:
    """Build a taskset-compatible CPU list string for `count` cores.

    Examples: count=2 -> "0-1"; count=4 offset=4 -> "4-7".
    """
    if available is not None:
        count = min(count, available - offset)
    if count <= 0:
        return "0"
    start = offset
    end = offset + count - 1
    if start == end:
        return str(start)
    return f"{start}-{end}"


def with_taskset(cmd: List[str], cores: Optional[str]) -> List[str]:
    """Prepend taskset to a command if a core mask is provided."""
    if not cores:
        return cmd
    return ["taskset", "-c", cores, *cmd]


def _raise_nofile_limit() -> None:
    """preexec_fn: raise soft RLIMIT_NOFILE to the hard limit for the child.

    mudud keeps ~4 fds per relation file (2 .dat + 2 WAL chunks); with
    warehouse-partitioned TPC-C this exceeds the common 1024 default.
    taskset execs the real binary in-process, so the raised limit survives.
    """
    soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
    if soft < hard:
        resource.setrlimit(resource.RLIMIT_NOFILE, (hard, hard))


def _log_child_nofile_limit(pid: int, label: str) -> None:
    """Log the child's effective Max open files limit from /proc for diagnosis."""
    try:
        for line in Path(f"/proc/{pid}/limits").read_text(encoding="utf-8").splitlines():
            if line.startswith("Max open files"):
                print(f"[info] {label} (pid {pid}): {line.strip()}", flush=True)
                return
    except OSError:
        pass


def system_memory_bytes() -> int:
    """Return total physical memory in bytes, parsed from /proc/meminfo."""
    try:
        for line in Path("/proc/meminfo").read_text(encoding="utf-8").splitlines():
            if line.startswith("MemTotal:"):
                # Format: "MemTotal:       65793084 kB"
                return int(line.split()[1]) * 1024
    except Exception:
        pass
    # Conservative fallback: 8 GiB.
    return 8 * 1024**3


def buffer_pool_size_mb(ratio: float) -> int:
    """Compute the buffer pool size in MB from the configured RAM ratio."""
    return max(128, int(system_memory_bytes() * ratio) // (1024 * 1024))


def server_max_connections(cfg: BenchConfig) -> int:
    """Server connection limit for the PostgreSQL/MySQL instances.

    The benchmark client pool opens `connections` sessions at once, the seed
    phase adds up to 16 shard sessions, and a few admin connections are used
    for setup/teardown. The historical fixed limit of 1000 fails the
    1024-connection sweep tier, so size the limit from the config.

    The margin must comfortably cover the seed/init connections that are
    still draining while the terminal pool ramps up: a +64 margin at the
    1024-connection tier caused intermittent ERROR 1040 "Too many
    connections" (all three 4-core runs failed this way).
    """
    return max(1000, max(cfg.connections) + 256)


# ---------------------------------------------------------------------------
# Result parsing (shared with script/bench/tpcc_benchmark.py)
# ---------------------------------------------------------------------------


# The SpacetimeDB client omits committed_tps from its summary line; the
# group is optional and the value is derived in parse_benchmark_output.
_TPCC_SUMMARY_RE = re.compile(
    r"tpcc benchmark mode=(\S+) "
    r"connections=(\d+) warehouses=(\d+) districts=(\d+) customers=(\d+) items=(\d+) "
    r"operations=(\d+) load_elapsed=([\d.]+)s txn_elapsed=([\d.]+)s total_elapsed=([\d.]+)s "
    r"throughput=([\d.]+) ops/s tps=([\d.]+) (?:committed_tps=([\d.]+) )?new_order_tps=([\d.]+) total_throughput=([\d.]+) ops/s "
    r"op_count=(\d+) abort_count=(\d+) abort_rate=([\d.]+)% "
    r"avg_latency=([\d.]+)ms min_latency=([\d.]+)ms max_latency=([\d.]+)ms "
    r"p50=([\d.]+)ms p90=([\d.]+)ms p99=([\d.]+)ms p999=([\d.]+)ms"
)


def _summary_committed_tps(m: "re.Match[str]") -> float:
    """committed_tps from a summary match, derived when the client omits it.

    The SpacetimeDB client does not print committed_tps; derive it with the
    same formula the main client uses, (op_count - abort_count)/txn_elapsed,
    i.e. tps * (op_count - abort_count) / op_count.
    """
    raw = m.group(13)
    if raw is not None:
        return float(raw)
    tps = float(m.group(12))
    op_count = int(m.group(16))
    abort_count = int(m.group(17))
    if op_count <= 0:
        return tps
    return tps * (op_count - abort_count) / op_count


def parse_benchmark_output(
    stdout: str,
    backend: str,
    mode: str,
    cores: int,
    connections: int,
    run: int,
    cfg: BenchConfig,
) -> Optional[TpccResult]:
    for line in stdout.splitlines():
        m = _TPCC_SUMMARY_RE.search(line)
        if m:
            return TpccResult(
                backend=backend,
                mode=m.group(1),
                cores=cores,
                connections=int(m.group(2)),
                run=run,
                warehouses=int(m.group(3)),
                districts=int(m.group(4)),
                customers=int(m.group(5)),
                items=int(m.group(6)),
                operations=int(m.group(7)),
                load_elapsed_sec=float(m.group(8)),
                txn_elapsed_sec=float(m.group(9)),
                total_elapsed_sec=float(m.group(10)),
                throughput=float(m.group(11)),
                tps=float(m.group(12)),
                committed_tps=_summary_committed_tps(m),
                new_order_tps=float(m.group(14)),
                total_throughput=float(m.group(15)),
                op_count=int(m.group(16)),
                abort_count=int(m.group(17)),
                abort_rate_pct=float(m.group(18)),
                avg_latency_ms=float(m.group(19)),
                min_latency_ms=float(m.group(20)),
                max_latency_ms=float(m.group(21)),
                p50_latency_ms=float(m.group(22)),
                p90_latency_ms=float(m.group(23)),
                p99_latency_ms=float(m.group(24)),
                p999_latency_ms=float(m.group(25)),
            )
    return None


# ---------------------------------------------------------------------------
# Backend abstraction
# ---------------------------------------------------------------------------


class BenchmarkBackend(ABC):
    """Abstract lifecycle for a benchmark target database."""

    name: str = ""
    mode: str = ""

    def __init__(self, project_root: Path, cfg: BenchConfig) -> None:
        self.project_root = project_root
        self.cfg = cfg
        self.data_dir: Optional[Path] = None
        self.log_dir: Optional[Path] = None
        self._processes: List[subprocess.Popen] = []

    @abstractmethod
    def start(self, cores: int) -> None:
        """Start the backend pinned to `cores` physical cores."""

    @abstractmethod
    def stop(self) -> None:
        """Stop the backend and clean up."""

    @abstractmethod
    def is_ready(self) -> bool:
        """Return True when the backend is ready to accept connections."""

    @abstractmethod
    def connection_env(self) -> Dict[str, str]:
        """Return environment variables (e.g. MUDU_CONNECTION) for the benchmark client."""

    @abstractmethod
    def benchmark_mode(self) -> str:
        """Return the --mode argument for tpcc-benchmark."""

    @abstractmethod
    def extra_args(self) -> List[str]:
        """Return extra CLI arguments for tpcc-benchmark."""

    def _remote_config(self) -> Optional[RemoteConfig]:
        """Remote settings when remote mode is enabled, else None."""
        if not remote_enabled(self.cfg):
            return None
        return RemoteConfig(self.cfg.remote)

    def _host(self) -> str:
        """Host the benchmark client connects to.

        Remote mode: the server host. Local mode: the backend's configured
        listen_ip when it has one (mudud, spacetimedb), else 127.0.0.1.
        """
        remote = self._remote_config()
        if remote is not None:
            return remote.server_host
        return str(getattr(self, "listen_ip", "127.0.0.1"))

    def _pick_port(self, remote_attr: str) -> int:
        """Listen port for this backend.

        Remote mode uses the fixed port from the remote section (the client
        must know it in advance); local mode picks an ephemeral free port.
        """
        remote = self._remote_config()
        if remote is not None:
            return int(getattr(remote, remote_attr))
        return get_free_port()

    def _select_ports(self, cores: int) -> None:
        """Choose the listen ports for this run (no server is started).

        Split out of start() so the remote wrapper (RemoteBackend) can
        populate the fixed remote ports on the client side without launching
        a local server. Default: nothing to select.
        """

    def readiness_port(self) -> int:
        """TCP port a client should wait on once the server is up (0 = none)."""
        return int(getattr(self, "port", 0))

    def prepare_client(self) -> None:
        """Build client-side artifacts needed before the benchmark runs.

        No-op for most backends; overridden where the client binary or
        packages must be built locally (spacetimedb client, mudud mpk),
        which matters in remote mode where start() runs on the server.
        """

    @staticmethod
    def effective_operations(configured: int, connections: int) -> int:
        """Scale total operations with the connection count.

        The client clamps its terminal count to min(connections, operations),
        so with a fixed small operation count the high-connection runs only
        spawn `operations` terminals running a single op each, and the timed
        window degenerates to a sub-second burst instead of a steady-state
        measurement. Guarantee at least 10 ops per terminal.
        """
        return max(configured, connections * 10)

    def benchmark_command(
        self,
        project_root: Path,
        connections: int,
        seckill_hotspot_percent: int = 0,
        think_time_ms: int = 0,
    ) -> List[str]:
        """Return the full benchmark client command for this backend.

        The default implementation builds the shared
        `cargo run --release -p tpcc --bin tpcc-benchmark` command (a debug
        client would bottleneck the measurement itself). Backends with their
        own client binary (e.g. SpacetimeDB) override this.

        `seckill_hotspot_percent` and `think_time_ms` are the per-run sweep
        values (hotspot only forwarded when > 0 and workload != "tpcc").
        """
        cmd = [
            "cargo",
            "run",
            "--release",
            "-p",
            "tpcc",
            "--features",
            "benchmark-runner",
            "--bin",
            "tpcc-benchmark",
            "--",
            "--mode",
            self.benchmark_mode(),
            "--warehouses",
            str(self.cfg.warehouses),
            "--districts-per-warehouse",
            str(self.cfg.districts),
            "--customers-per-district",
            str(self.cfg.customers),
            "--items",
            str(self.cfg.items),
            "--operation-count",
            str(self.effective_operations(self.cfg.operations, connections)),
            "--warmup-operations",
            str(self.cfg.warmup_operations),
            "--connection-count",
            str(connections),
            "--payment-percent",
            str(self.cfg.payment_percent),
            "--new-order-percent",
            str(self.cfg.new_order_percent),
        ]
        if self.cfg.perf_sample_rate > 0:
            cmd.extend(["--perf-sample-rate", str(self.cfg.perf_sample_rate)])
        if self.cfg.workload != "tpcc":
            cmd.extend(
                [
                    "--workload",
                    self.cfg.workload,
                    "--seckill-items",
                    str(self.cfg.seckill_items),
                    "--seckill-payload-bytes",
                    str(self.cfg.seckill_payload_bytes),
                ]
            )
            if seckill_hotspot_percent > 0:
                cmd.extend(
                    ["--seckill-hotspot-percent", str(seckill_hotspot_percent)]
                )
        if self.cfg.hot_rows_per_warehouse > 0:
            cmd.extend(
                ["--hot-rows-per-warehouse", str(self.cfg.hot_rows_per_warehouse)]
            )
        if self.cfg.order_lines > 0:
            cmd.extend(["--order-lines", str(self.cfg.order_lines)])
        # Always passed (even 0) so an explicit think_times_ms: [0] overrides
        # the client's built-in TPC-C default.
        cmd.extend(["--think-time-ms", str(think_time_ms)])
        # Port sharding (tcp_multi_port=true) is enabled on the server, and
        # extra_args() passes --tcp-multi-port so both benchmark modes
        # distribute their connections across the per-worker ports.
        cmd.extend(self.extra_args())
        return cmd

    def _ensure_work_dir(self) -> None:
        """Recreate the run work dir if something external removed it mid-run."""
        assert self.cfg.work_dir is not None
        if not self.cfg.work_dir.is_dir():
            print(
                f"[warn] work directory {self.cfg.work_dir} disappeared; "
                "recreating it",
                file=sys.stderr,
            )
            self.cfg.work_dir.mkdir(parents=True, exist_ok=True)

    def cleanup(self) -> None:
        for proc in self._processes:
            try:
                if proc.poll() is None:
                    proc.terminate()
                    proc.wait(timeout=5)
            except Exception:
                try:
                    proc.kill()
                except Exception:
                    pass
        self._processes.clear()
        if not self.cfg.keep_data and self.data_dir is not None and self.data_dir.exists():
            shutil.rmtree(self.data_dir, ignore_errors=True)

    def _start_process(
        self,
        cmd: List[str],
        log_name: str,
        env: Optional[Dict[str, str]] = None,
        cwd: Optional[Path] = None,
    ) -> subprocess.Popen:
        log_path = (self.log_dir or self.data_dir or Path.cwd()) / log_name
        log_path.parent.mkdir(parents=True, exist_ok=True)
        log_file = open(log_path, "w")
        proc = subprocess.Popen(
            cmd,
            cwd=str(cwd) if cwd else str(self.project_root),
            stdout=log_file,
            stderr=subprocess.STDOUT,
            text=True,
            env=env,
        )
        self._processes.append(proc)
        return proc


# ---------------------------------------------------------------------------
# PostgreSQL backend
# ---------------------------------------------------------------------------


class PostgresBackend(BenchmarkBackend):
    name = "postgres"
    mode = "interactive"

    def __init__(self, project_root: Path, cfg: BenchConfig) -> None:
        super().__init__(project_root, cfg)
        self.pgcfg = cfg.postgres
        self.initdb = Path(self.pgcfg.get("initdb", "initdb"))
        self.pg_ctl = Path(self.pgcfg.get("pg_ctl", "pg_ctl"))
        self.psql = Path(self.pgcfg.get("psql", "psql"))
        self.user = self.pgcfg.get("user", "postgres")
        self.password = self.pgcfg.get("password", "postgres")
        self.db_name = self.pgcfg.get("database", "tpcc")
        self.port = 0

    def _select_ports(self, cores: int) -> None:
        self.port = self._pick_port("postgres_port")

    def start(self, cores: int) -> None:
        self._select_ports(cores)
        remote = self._remote_config()
        assert self.cfg.work_dir is not None
        self._ensure_work_dir()
        self.data_dir = Path(
            tempfile.mkdtemp(dir=self.cfg.work_dir, prefix="tpcc_pg_")
        )

        # initdb requires the data directory to be empty, so do not create
        # subdirectories here.
        run_cmd(
            [
                str(self.initdb),
                "-D",
                str(self.data_dir),
                "--username",
                self.user,
                "--auth",
                "trust",
                "--no-locale",
                "--encoding=UTF8",
            ],
            cwd=self.project_root,
            timeout=120.0,
        )

        if remote is not None:
            # initdb --auth=trust only writes host entries for 127.0.0.1/32
            # and ::1/128; remote mode needs the client machine to connect,
            # so trust all hosts (benchmark instances are throwaway).
            pg_hba = self.data_dir / "pg_hba.conf"
            with pg_hba.open("a", encoding="utf-8") as f:
                f.write("\n# Added by bench_cross_db.py remote mode\n")
                f.write("host\tall\tall\t0.0.0.0/0\ttrust\n")
                f.write("host\tall\tall\t::/0\ttrust\n")

        # create log dir only after initdb succeeds
        self.log_dir = self.data_dir / "logs"
        self.log_dir.mkdir(parents=True, exist_ok=True)

        # start server
        core_mask = build_core_mask(cores)
        shared_buffers_mb = buffer_pool_size_mb(self.cfg.buffer_pool_ratio)
        max_connections = server_max_connections(self.cfg)
        # Remote mode must accept connections from the client machine.
        listen_addr = "0.0.0.0" if remote is not None else "127.0.0.1"
        start_cmd = [
            str(self.pg_ctl),
            "-D",
            str(self.data_dir),
            "-l",
            str(self.log_dir / "server.log"),
            "start",
            "-o",
            (
                f"-h {listen_addr} -p {self.port}"
                f" -c max_connections={max_connections}"
                # Use READ COMMITTED so concurrent same-row UPDATEs queue on
                # row locks, matching MySQL's REPEATABLE READ (InnoDB locking
                # reads) and MuduDB's conflict handling. PostgreSQL's
                # REPEATABLE READ is snapshot isolation and instead fails
                # every concurrent update with 40001 "could not serialize
                # access due to concurrent update", which inflated the TPC-C
                # abort rate to 50-84% on the district hot row.
                f" -c default_transaction_isolation='read committed'"
                f" -c shared_buffers={shared_buffers_mb}MB"
                f" -c unix_socket_directories={self.data_dir}"
            ),
        ]
        try:
            run_cmd(
                with_taskset(start_cmd, core_mask),
                cwd=self.project_root,
                timeout=120.0,
            )
        except RuntimeError:
            log_file = self.log_dir / "server.log"
            if log_file.exists():
                print(
                    f"[error] PostgreSQL server log ({log_file}):",
                    file=sys.stderr,
                )
                print("-" * 60, file=sys.stderr)
                print(log_file.read_text(encoding="utf-8", errors="replace"), file=sys.stderr)
                print("-" * 60, file=sys.stderr)
            raise

        if not wait_for_port("127.0.0.1", self.port, timeout=60.0):
            raise RuntimeError("PostgreSQL failed to start")

        # Create the database; the Rust benchmark binary creates the schema
        # itself via init_schema_sync, so do not run DDL here.
        conn = f"-h 127.0.0.1 -p {self.port} -U {self.user}"
        run_cmd(
            [str(self.psql), *conn.split(), "-c", f"CREATE DATABASE {self.db_name};"],
            cwd=self.project_root,
            timeout=60.0,
        )

    def stop(self) -> None:
        if self.data_dir is None:
            return
        try:
            run_cmd(
                [str(self.pg_ctl), "-D", str(self.data_dir), "stop", "-m", "fast"],
                cwd=self.project_root,
                timeout=60.0,
                check=False,
            )
        finally:
            self.cleanup()

    def is_ready(self) -> bool:
        return self.port != 0 and wait_for_port("127.0.0.1", self.port, timeout=1.0)

    def connection_env(self) -> Dict[str, str]:
        password_part = f":{self.password}" if self.password else ""
        return {
            "MUDU_CONNECTION": (
                f"postgres://{self.user}{password_part}@{self._host()}:{self.port}/{self.db_name}"
            )
        }

    def benchmark_mode(self) -> str:
        return "interactive"

    def extra_args(self) -> List[str]:
        return []


class PostgresProcedureBackend(PostgresBackend):
    """PostgreSQL in stored-procedure mode (label `postgres-n`).

    Runs the exact same server lifecycle and configuration as the interactive
    postgres backend; only the benchmark client mode differs: the client
    installs PL/pgSQL procedures (example/tpcc/sql/procedures_postgres.sql)
    into the fresh database and invokes one function per transaction instead
    of issuing interactive statements.
    """

    name = "postgres-procedure"
    mode = "pg-procedure"

    def benchmark_mode(self) -> str:
        return "pg-procedure"


# ---------------------------------------------------------------------------
# MySQL backend
# ---------------------------------------------------------------------------


class MySqlBackend(BenchmarkBackend):
    name = "mysql"
    mode = "interactive"

    def __init__(self, project_root: Path, cfg: BenchConfig) -> None:
        super().__init__(project_root, cfg)
        self.mycfg = cfg.mysql
        self.mysqld = Path(self.mycfg.get("mysqld", "mysqld"))
        self.mysqladmin = Path(self.mycfg.get("mysqladmin", "mysqladmin"))
        self.mysql = Path(self.mycfg.get("mysql", "mysql"))
        self.user = self.mycfg.get("user", "root")
        self.password = self.mycfg.get("password", "")
        self.db_name = self.mycfg.get("database", "tpcc")
        self.port = 0
        self.socket_path: Optional[Path] = None

    def _work_parent(self) -> Path:
        """Directory the MySQL data directory is created in.

        Ubuntu's mysqld is confined by AppArmor (see
        /etc/apparmor.d/usr.sbin.mysqld) and may only write under
        /var/lib/mysql and the user-tmp abstraction paths (/tmp, /var/tmp).
        When that profile exists and no explicit mysql.work_dir is set, fall
        back to /var/tmp so initialization succeeds; /var/tmp is a real
        filesystem on typical installs (unlike tmpfs-backed /tmp).
        """
        override = self.mycfg.get("work_dir")
        if override:
            path = Path(override)
            path.mkdir(parents=True, exist_ok=True)
            return path
        profile = Path("/etc/apparmor.d/usr.sbin.mysqld")
        if profile.exists() and Path("/var/tmp").is_dir():
            if not MySqlBackend._apparmor_warning_printed:
                print(
                    "[warn] mysqld AppArmor profile detected; MySQL data will "
                    "be placed under /var/tmp (the profile forbids the "
                    "project work dir). Set mysql.work_dir to choose an "
                    "AppArmor-allowed location on the device you want to "
                    "benchmark."
                )
                MySqlBackend._apparmor_warning_printed = True
            return Path("/var/tmp")
        assert self.cfg.work_dir is not None
        return self.cfg.work_dir

    _apparmor_warning_printed = False

    def _select_ports(self, cores: int) -> None:
        self.port = self._pick_port("mysql_port")

    def start(self, cores: int) -> None:
        self._select_ports(cores)
        remote = self._remote_config()
        self._ensure_work_dir()
        self.data_dir = Path(
            tempfile.mkdtemp(dir=self._work_parent(), prefix="tpcc_mysql_")
        )
        self.socket_path = self.data_dir / "mysql.sock"

        # initialize data directory (insecure -> empty root password).
        # Keep logs inside the temp data dir; do not create subdirectories here,
        # because --initialize-insecure requires an empty data directory.
        run_cmd(
            [
                str(self.mysqld),
                "--initialize-insecure",
                f"--datadir={self.data_dir}",
                f"--log-error={self.data_dir}/initialize.log",
            ],
            cwd=self.project_root,
            timeout=120.0,
        )

        # create log dir only after initialization succeeds
        self.log_dir = self.data_dir / "logs"
        self.log_dir.mkdir(parents=True, exist_ok=True)

        # start server
        core_mask = build_core_mask(cores)
        buffer_pool_mb = buffer_pool_size_mb(self.cfg.buffer_pool_ratio)
        # Remote mode must accept connections from the client machine.
        bind_address = "0.0.0.0" if remote is not None else "127.0.0.1"
        start_cmd = [
            str(self.mysqld),
            f"--datadir={self.data_dir}",
            f"--port={self.port}",
            f"--bind-address={bind_address}",
            f"--socket={self.socket_path}",
            f"--log-error={self.log_dir}/error.log",
            f"--max_connections={server_max_connections(self.cfg)}",
            # Explicit REPEATABLE READ to align with PostgreSQL (snapshot
            # isolation via REPEATABLE READ) and MuduDB (native SI). This is
            # InnoDB's default; set it explicitly for clarity.
            "--transaction-isolation=REPEATABLE-READ",
            f"--innodb_buffer_pool_size={buffer_pool_mb}M",
        ]
        self._start_process(
            with_taskset(start_cmd, core_mask),
            log_name="server.log",
        )

        if not wait_for_port("127.0.0.1", self.port, timeout=60.0):
            raise RuntimeError("MySQL failed to start")

        # Create the database; the Rust benchmark binary creates the schema
        # itself via init_schema_sync, so do not run DDL here.
        auth = [f"-u{self.user}"]
        if self.password:
            auth.append(f"-p{self.password}")
        conn = [*auth, "-h127.0.0.1", f"-P{self.port}"]
        run_cmd(
            [str(self.mysql), *conn, "-e", f"CREATE DATABASE {self.db_name};"],
            cwd=self.project_root,
            timeout=60.0,
        )

        if remote is not None:
            # root@'%' does not exist on an --initialize-insecure instance;
            # create a dedicated account reachable from the client machine.
            run_cmd(
                [
                    str(self.mysql),
                    *conn,
                    "-e",
                    "CREATE USER IF NOT EXISTS 'bench'@'%' "
                    "IDENTIFIED BY 'bench';"
                    " GRANT ALL PRIVILEGES ON *.* TO 'bench'@'%';"
                    " FLUSH PRIVILEGES;",
                ],
                cwd=self.project_root,
                timeout=60.0,
            )

    def stop(self) -> None:
        if self.data_dir is None:
            return
        try:
            auth = [f"-u{self.user}"]
            if self.password:
                auth.append(f"-p{self.password}")
            run_cmd(
                [
                    str(self.mysqladmin),
                    *auth,
                    "-h127.0.0.1",
                    f"-P{self.port}",
                    "shutdown",
                ],
                cwd=self.project_root,
                timeout=60.0,
                check=False,
            )
        finally:
            self.cleanup()

    def is_ready(self) -> bool:
        return self.port != 0 and wait_for_port("127.0.0.1", self.port, timeout=1.0)

    def connection_env(self) -> Dict[str, str]:
        # Remote mode connects as the 'bench'@'%' account created in start();
        # the configured user (default root) only exists for localhost.
        if self._remote_config() is not None:
            user, password = "bench", "bench"
        else:
            user, password = self.user, self.password
        password_part = f":{password}" if password else ""
        return {
            "MUDU_CONNECTION": (
                f"mysql://{user}{password_part}@{self._host()}:{self.port}/{self.db_name}"
            )
        }

    def benchmark_mode(self) -> str:
        return "interactive"

    def extra_args(self) -> List[str]:
        return []


# ---------------------------------------------------------------------------
# MuduDB backend
# ---------------------------------------------------------------------------


def find_mudud_binary(project_root: Path) -> Optional[Path]:
    """Locate the release mudud binary; benchmarks never use a debug build."""
    p = project_root / "target" / "release" / "mudud"
    return p if p.exists() else None


def ensure_release_mudud(project_root: Path) -> Path:
    """Return the release mudud binary, building it first when missing.

    Benchmarks must run against a release server: a debug build is 5-10x
    slower and silently produced an entire bogus result set once. A debug
    binary is therefore an error-level complaint, never a fallback. The
    build runs on whatever machine executes this code — in remote mode the
    --server-run agent runs it on the server host against the sources synced
    from the client (--setup-remote), which is exactly where the server must
    be built.
    """
    release_bin = find_mudud_binary(project_root)
    if release_bin is not None:
        return release_bin
    debug_bin = project_root / "target" / "debug" / "mudud"
    if debug_bin.exists():
        print(
            f"[error] found debug mudud binary {debug_bin}; debug builds are "
            "unusable for benchmarks (numbers come out far below release).",
            file=sys.stderr,
        )
    print(
        "[info] no release mudud binary found; building it now with "
        "`cargo build --release -p mudud` ...",
        file=sys.stderr,
    )
    run_cmd(
        ["cargo", "build", "--release", "-p", "mudud"],
        cwd=project_root,
        timeout=3600.0,
    )
    release_bin = find_mudud_binary(project_root)
    if release_bin is None:
        raise RuntimeError(
            "cargo build --release -p mudud finished but "
            f"{project_root / 'target' / 'release' / 'mudud'} is still missing"
        )
    return release_bin


def mudud_binary_profile(mudud_bin: Path) -> str:
    """Build profile of a mudud binary path ('release' or 'debug')."""
    return mudud_bin.parent.name


# Cache for newest_source_mtime: the scan walks the whole workspace, so do it
# once per project root per process instead of on every backend start.
_SOURCE_MTIME_CACHE: Dict[Path, float] = {}


def newest_source_mtime(project_root: Path) -> float:
    """Return the newest mtime across workspace sources that can affect mudud."""
    cached = _SOURCE_MTIME_CACHE.get(project_root)
    if cached is not None:
        return cached
    newest = 0.0
    # Skip build outputs, scratch dirs, and generated source directories:
    # generated files are refreshed by builds (e.g. ts_const from
    # tree-sitter grammars, tpcc/src/generated from cargo make) and would
    # otherwise make the binary look stale after every unrelated build.
    skip = {
        "target",
        ".git",
        "bench_cross_db_work",
        "bench_cross_db_results",
        "ts_const",
        "generated",
        "artifact",
    }
    # Transpiler output trees whose names are too generic for the
    # name-based skip list above (regenerated by cargo make tasks), plus the
    # tpcc benchmark itself (it is not linked into the mudud binary).
    skip_trees = [
        project_root / "example" / "tpcc" / "src" / "rust",
        project_root / "example" / "tpcc" / "src" / "bin",
    ]
    for dirpath, dirnames, filenames in os.walk(project_root):
        dirnames[:] = [d for d in dirnames if d not in skip]
        if any(Path(dirpath).is_relative_to(tree) for tree in skip_trees):
            continue
        for name in filenames:
            if name.endswith((".rs", ".js", ".toml")):
                try:
                    mtime = (Path(dirpath) / name).stat().st_mtime
                except OSError:
                    continue
                if mtime > newest:
                    newest = mtime
    _SOURCE_MTIME_CACHE[project_root] = newest
    return newest


def warn_if_mudud_stale(project_root: Path, mudud_bin: Path) -> None:
    """Warn when the chosen mudud binary is older than workspace sources.

    Running a stale server binary silently benchmarks (or fails with) old
    code; benchmarks only ever use the release build (find_mudud_binary).
    """
    try:
        bin_mtime = mudud_bin.stat().st_mtime
    except OSError:
        return
    newest = newest_source_mtime(project_root)
    if newest > bin_mtime:
        print(
            f"[warn] mudud binary {mudud_bin} is older than workspace sources "
            f"(binary {time.strftime('%F %T', time.localtime(bin_mtime))}, "
            f"newest source {time.strftime('%F %T', time.localtime(newest))}); "
            "rebuild it (cargo build --release -p mudud) to avoid benchmarking "
            "stale code",
            file=sys.stderr,
        )


def mudud_supports_cfg_flag(mudud_bin: Path) -> bool:
    """Check whether the mudud binary accepts --cfg on the serve subcommand."""
    try:
        result = subprocess.run(
            [str(mudud_bin), "serve", "--help"],
            capture_output=True,
            text=True,
            timeout=3.0,
        )
        return result.returncode == 0 and "--cfg" in (result.stdout + result.stderr)
    except Exception:
        return False


class MududbBackend(BenchmarkBackend):
    name = "mududb"

    def __init__(
        self,
        project_root: Path,
        cfg: BenchConfig,
        mode: str,
        server_mode: Optional[str] = None,
        name: str = "mududb",
    ) -> None:
        super().__init__(project_root, cfg)
        self.name = name
        self.mode = mode  # "interactive" or "procedure"
        self.mcfg = cfg.mudud
        # The server mode comes from the backend's `mudud.modes` entry;
        # "iouring" is the built-in default when none was specified.
        self.server_mode = server_mode or "iouring"
        self.listen_ip = self.mcfg.get("listen_ip", "127.0.0.1")
        # Allocated in _select_ports() (called at the top of start()): with
        # tcp_multi_port=true the server binds one port per worker
        # (tcp_port .. tcp_port+cores-1), so the block size depends on the
        # core count passed to start().
        self.http_port = 0
        self.tcp_port = 0
        # Built lazily by _mpk(): a remote-mode server agent (--server-run)
        # must not try to build wasm artifacts on the server machine.
        self.mpk_path: Optional[Path] = None
        self._mudud_proc: Optional[subprocess.Popen] = None
        self._log_file = None
        # Set in start(): "<path> (<profile>)" of the server binary actually
        # launched, recorded into each TpccResult so a result file shows which
        # binary produced its numbers.
        self.server_binary = ""

    def _build_mpk(self) -> Path:
        tpcc_dir = self.project_root / "example" / "tpcc"
        run_cmd(["cargo", "make", "package-partitioned"], cwd=tpcc_dir, timeout=600.0)
        mpk = (
            self.project_root
            / "target"
            / "wasm32-wasip2"
            / "release"
            / "tpcc_partitioned.mpk"
        )
        if not mpk.exists():
            raise RuntimeError(f"Expected mpk not found at {mpk}")
        return mpk

    def _mpk(self) -> Optional[Path]:
        """Path of the partitioned mpk for procedure mode, built on first use."""
        if self.mode != "procedure":
            return None
        if self.mpk_path is None:
            self.mpk_path = self._build_mpk()
        return self.mpk_path

    def _select_ports(self, cores: int) -> None:
        self.http_port = self._pick_port("mudud_http_port")
        remote = self._remote_config()
        if remote is not None:
            # Fixed base port; the server binds mudud_tcp_port..+cores-1.
            self.tcp_port = remote.mudud_tcp_port
        else:
            self.tcp_port = get_free_port_block(cores, self.listen_ip)

    def readiness_port(self) -> int:
        return self.http_port

    def prepare_client(self) -> None:
        # The mpk is pushed to the server over HTTP by the benchmark client,
        # so it must be built on the client machine even in remote mode.
        self._mpk()

    def _write_config(self, cores: int) -> Path:
        assert self.data_dir is not None
        mpk_dir = self.data_dir / "mpk"
        mpk_dir.mkdir(parents=True, exist_ok=True)

        mode_map = {
            "legacy": "Legacy",
            "iouring": "IOUring",
            "io_uring": "IOUring",
            "tokio": "Tokio",
        }
        server_mode_str = mode_map.get(self.server_mode.lower(), "IOUring")

        cfg_path = self.data_dir / "mudud.cfg"
        config_text = f"""mpk_path = "{mpk_dir}"
db_path = "{self.data_dir}"
listen_ip = "{self.listen_ip}"
http_listen_port = {self.http_port}
pg_listen_port = 0
tcp_listen_port = {self.tcp_port}
server_mode = "{server_mode_str}"
tcp_multi_port = true
worker_threads = {cores}
io_uring_ring_entries = {self.mcfg.get("ring_entries", 1024)}
io_uring_accept_multishot = true
io_uring_recv_multishot = true
io_uring_enable_fixed_buffers = false
io_uring_enable_fixed_files = false
routing_mode = "ConnectionId"
enable_async = true
http_worker_threads = 1
"""
        wal_max_wait_us = self.mcfg.get("wal_flush_max_wait_us")
        if wal_max_wait_us is not None:
            config_text += f"wal_flush_max_wait_us = {int(wal_max_wait_us)}\n"
        wal_sync_mode = self.mcfg.get("wal_sync_mode")
        if wal_sync_mode is not None:
            config_text += f'wal_sync_mode = "{wal_sync_mode}"\n'
        wal_sync_interval_ms = self.mcfg.get("wal_sync_interval_ms")
        if wal_sync_interval_ms is not None:
            config_text += f"wal_sync_interval_ms = {int(wal_sync_interval_ms)}\n"
        page_size = self.mcfg.get("page_size")
        if page_size is not None:
            config_text += f"page_size = {int(page_size)}\n"
        cfg_path.write_text(config_text, encoding="utf-8")
        return cfg_path

    def start(self, cores: int) -> None:
        assert self.cfg.work_dir is not None
        self._ensure_work_dir()
        self.data_dir = Path(
            tempfile.mkdtemp(dir=self.cfg.work_dir, prefix="tpcc_mudud_")
        )
        self.log_dir = self.data_dir / "logs"
        self.log_dir.mkdir(parents=True, exist_ok=True)

        self._select_ports(cores)
        cfg_path = self._write_config(cores)

        mudud_bin = ensure_release_mudud(self.project_root)
        profile = mudud_binary_profile(mudud_bin)
        if profile != "release":
            # Defensive: ensure_release_mudud only returns the release path.
            # A debug server is typically 5-10x slower; silently benchmarking
            # it once produced an entire bogus result set.
            raise RuntimeError(
                f"mudud binary {mudud_bin} is a {profile} build; benchmarks "
                "require a release build (`cargo build --release -p mudud`)"
            )
        self.server_binary = f"{mudud_bin} ({profile})"
        bin_mtime = time.strftime(
            "%F %T", time.localtime(mudud_bin.stat().st_mtime)
        )
        print(
            f"[info] mudud binary: {mudud_bin} "
            f"(profile={profile}, built {bin_mtime})"
        )
        warn_if_mudud_stale(self.project_root, mudud_bin)
        if mudud_supports_cfg_flag(mudud_bin):
            cmd = [str(mudud_bin), "serve", "--cfg", str(cfg_path)]
        else:
            cmd = [str(mudud_bin), "serve", str(cfg_path)]

        core_mask = build_core_mask(cores)
        log_path = self.log_dir / "server.log"
        log_file = open(log_path, "w")
        proc = subprocess.Popen(
            with_taskset(cmd, core_mask),
            cwd=str(self.project_root),
            stdout=log_file,
            stderr=subprocess.STDOUT,
            text=True,
            preexec_fn=_raise_nofile_limit,
        )
        self._mudud_proc = proc
        self._log_file = log_file
        self._processes.append(proc)
        _log_child_nofile_limit(proc.pid, "mudud")

        http_ready = wait_for_port(self.listen_ip, self.http_port, timeout=60.0)
        # tcp_multi_port binds one listener per worker at consecutive ports;
        # wait for all of them so a worker bind failure (e.g. port already in
        # use) fails fast here instead of surfacing later as "server is not
        # ready" on the client.
        worker_ports = range(self.tcp_port, self.tcp_port + cores)
        tcp_ready = all(
            wait_for_port(self.listen_ip, port, timeout=60.0)
            for port in worker_ports
        )
        if not http_ready or not tcp_ready:
            log_file.flush()
            tail = log_path.read_text(encoding="utf-8", errors="replace")[-2000:]
            raise RuntimeError(
                f"MuduDB failed to bind ports (http={http_ready}, tcp={tcp_ready})\n{tail}"
            )
        # Give the kernel a moment to finish internal setup (worker registry,
        # filesystem metadata, etc.) before clients connect.
        time.sleep(1.0)

    def stop(self) -> None:
        try:
            if self._mudud_proc is not None and self._mudud_proc.poll() is None:
                self._mudud_proc.send_signal(signal.SIGTERM)
                try:
                    self._mudud_proc.wait(timeout=10.0)
                except subprocess.TimeoutExpired:
                    self._mudud_proc.kill()
                    self._mudud_proc.wait()
        finally:
            if self._log_file is not None:
                self._log_file.close()
            self.cleanup()

    def is_ready(self) -> bool:
        return (
            self.http_port != 0
            and self.tcp_port != 0
            and wait_for_port(self.listen_ip, self.http_port, timeout=1.0)
            and wait_for_port(self.listen_ip, self.tcp_port, timeout=1.0)
        )

    def connection_env(self) -> Dict[str, str]:
        # Both modes get MUDU_CONNECTION: the sync adapter is used for
        # schema init and seeding (statement-routed, works across
        # partitions), procedures are invoked over TCP afterwards.
        host = self._host()
        return {
            "MUDU_CONNECTION": (
                f"mudud://{host}:{self.tcp_port}/tpcc"
                f"?http_addr={host}:{self.http_port}"
            )
        }

    def benchmark_mode(self) -> str:
        if self.mode == "procedure":
            return "stored-procedure"
        return self.mode

    def extra_args(self) -> List[str]:
        host = self._host()
        args: List[str] = [
            "--tcp-addr",
            f"{host}:{self.tcp_port}",
            "--http-addr",
            f"{host}:{self.http_port}",
            # The server config always enables tcp_multi_port; let the
            # benchmark client spread connections across the per-worker
            # ports instead of funnelling everything into worker 0.
            "--tcp-multi-port",
        ]
        if self.mode == "interactive":
            # Warehouse-partition the dataset so each worker owns the
            # warehouses placed on it instead of funnelling all data access
            # into worker 0.
            args.append("--warehouse-partitioned")
        mpk = self._mpk()
        if mpk is not None:
            # The partitioned package carries the partitioned DDL; the
            # benchmark only needs to create the placement and invoke the
            # *_partitioned procedures.
            args.extend(["--mpk", str(mpk), "--warehouse-partitioned"])
        partition_count = self.mcfg.get("partition_count")
        if partition_count:
            args.extend(["--partition-count", str(partition_count)])
        return args


# ---------------------------------------------------------------------------
# SpacetimeDB backend
# ---------------------------------------------------------------------------


class SpacetimeDbBackend(BenchmarkBackend):
    name = "spacetimedb"
    mode = "spacetimedb-reducer"

    def __init__(self, project_root: Path, cfg: BenchConfig) -> None:
        super().__init__(project_root, cfg)
        scfg = cfg.spacetimedb
        self.version = str(scfg.get("version", "1.12.0"))
        self.cli_path = str(scfg.get("cli_path", "") or "")
        install_dir = str(scfg.get("install_dir", "") or "")
        self.install_dir = (
            Path(install_dir)
            if install_dir
            else project_root / "bench_cross_db_work" / "spacetime-cli"
        )
        self.listen_ip = scfg.get("listen_ip", "127.0.0.1")
        self.port = 0
        self._cli_cmd: Optional[List[str]] = None

    def _stdb_dir(self) -> Path:
        return self.project_root / "example" / "tpcc" / "spacetimedb"

    def _module_dir(self) -> Path:
        return self._stdb_dir() / "module"

    def _client_dir(self) -> Path:
        return self._stdb_dir() / "client"

    def _client_bin(self) -> Path:
        return self._client_dir() / "target" / "release" / "tpcc-stdb-benchmark"

    def _find_installed_cli(self) -> Optional[Path]:
        """Locate a usable spacetimedb-cli binary under install_dir.

        Prefers the pinned `spacetimedb-cli` from the release tarball. The
        `spacetime` updater wrapper created by the official install script is
        deliberately avoided: it re-execs bin/current and breaks when the
        pinned version is not the updater's default.
        """
        candidates = [
            self.install_dir / "versions" / self.version / "spacetimedb-cli",
            self.install_dir / "spacetimedb-cli",
        ]
        candidates.extend(sorted(self.install_dir.glob("versions/*/spacetimedb-cli")))
        for c in candidates:
            if c.exists() and os.access(c, os.X_OK):
                return c
        return None

    def _resolve_cli(self) -> List[str]:
        """Resolve the spacetime CLI command prefix.

        An externally provided `cli_path` is used as-is; otherwise the CLI
        under `install_dir` is used, installing it first when missing. No
        `--root-dir` is appended: spacetimedb-cli derives its root directory
        from the binary location, and an explicit --root-dir would make some
        subcommands re-exec `<root>/spacetime`, which may not exist.
        """
        if self._cli_cmd is not None:
            return self._cli_cmd
        if self.cli_path:
            cli = Path(self.cli_path)
            if not (cli.exists() and os.access(cli, os.X_OK)):
                raise RuntimeError(
                    f"spacetimedb.cli_path is set but not executable: {cli}"
                )
            self._cli_cmd = [str(cli)]
            return self._cli_cmd
        cli = self._find_installed_cli()
        if cli is None:
            self._install_cli()
            cli = self._find_installed_cli()
            if cli is None:
                raise RuntimeError(
                    "SpacetimeDB CLI install did not produce a spacetimedb-cli "
                    f"binary under {self.install_dir}. "
                    "Set spacetimedb.cli_path to an existing `spacetime` "
                    "binary to skip auto-install."
                )
        self._cli_cmd = [str(cli)]
        return self._cli_cmd

    def _install_cli(self) -> None:
        """Install the pinned SpacetimeDB CLI into install_dir.

        Primary path: download the pinned release tarball from GitHub (the
        tarball ships `spacetimedb-cli`/`spacetimedb-standalone` directly).
        Fallback: the official install script plus `version install/use`.
        """
        self.install_dir.mkdir(parents=True, exist_ok=True)
        errors: List[str] = []

        url = (
            "https://github.com/clockworklabs/SpacetimeDB/releases/download/"
            f"v{self.version}/spacetime-x86_64-unknown-linux-gnu.tar.gz"
        )
        dest = self.install_dir / "versions" / self.version
        tarball = self.install_dir / f"spacetime-{self.version}.tar.gz"
        try:
            dest.mkdir(parents=True, exist_ok=True)
            run_cmd(
                [
                    "curl", "-fL", "--retry", "5", "--retry-delay", "2",
                    "-o", str(tarball), url,
                ],
                timeout=900.0,
            )
            run_cmd(["tar", "-xzf", str(tarball), "-C", str(dest)], timeout=300.0)
            tarball.unlink(missing_ok=True)
            # The tarball may nest the binaries in a subdirectory; flatten.
            for name in ("spacetimedb-cli", "spacetimedb-standalone"):
                for nested in sorted(dest.glob(f"**/{name}")):
                    if nested.parent != dest:
                        shutil.move(str(nested), str(dest / name))
            if self._find_installed_cli() is not None:
                return
            errors.append(f"tarball from {url} extracted but no spacetimedb-cli found")
        except Exception as e:
            errors.append(f"release tarball: {e}")

        install_script = (
            "curl --proto '=https' --tlsv1.2 -sSf https://install.spacetimedb.com"
            f" | sh -s -- --root-dir {shlex.quote(str(self.install_dir))} -y"
        )
        try:
            run_cmd(["sh", "-c", install_script], timeout=600.0)
            wrapper = str(self.install_dir / "spacetime")
            root = str(self.install_dir)
            run_cmd(
                [wrapper, f"--root-dir={root}", "version", "install", self.version],
                timeout=900.0,
            )
            run_cmd(
                [wrapper, f"--root-dir={root}", "version", "use", self.version],
                timeout=120.0,
            )
            if self._find_installed_cli() is not None:
                return
            errors.append("install script completed but no spacetimedb-cli found")
        except Exception as e:
            errors.append(f"install script: {e}")

        raise RuntimeError(
            f"Failed to install SpacetimeDB CLI {self.version} into "
            f"{self.install_dir}:\n"
            + "\n".join(f"  - {err}" for err in errors)
            + "\nSet spacetimedb.cli_path to an existing "
            "`spacetime`/`spacetimedb-cli` binary to skip auto-install."
        )

    def _ensure_built(self) -> None:
        """Build the module and client on first use; on later uses, refresh
        the client with an incremental cargo build so source changes are
        never served from a stale binary."""
        client_bin = self._client_bin()
        client_dir = self._client_dir()
        if client_bin.exists():
            # Incremental refresh: a no-op when fresh, but picks up source
            # edits (e.g. new CLI flags) that the old binary would reject.
            run_cmd(
                ["cargo", "build", "--release"], cwd=client_dir, timeout=1800.0
            )
            return
        module_dir = self._module_dir()
        for d in (module_dir, client_dir):
            if not (d / "Cargo.toml").exists():
                raise RuntimeError(
                    f"SpacetimeDB crate not found at {d} (missing Cargo.toml). "
                    "The Rust module/client under example/tpcc/spacetimedb/ "
                    "is not ready yet."
                )

        installed = run_cmd(["rustup", "target", "list", "--installed"], timeout=60.0)
        if "wasm32-unknown-unknown" not in installed.stdout.split():
            run_cmd(
                ["rustup", "target", "add", "wasm32-unknown-unknown"],
                timeout=300.0,
            )
        run_cmd(
            ["cargo", "build", "--release", "--target", "wasm32-unknown-unknown"],
            cwd=module_dir,
            timeout=1800.0,
        )

        cli = self._resolve_cli()
        bindings_dir = client_dir / "src" / "module_bindings"
        bindings_dir.mkdir(parents=True, exist_ok=True)
        # 1.12 uses --project-path; newer versions renamed it --module-path.
        gen_cmd = [
            *cli,
            "generate",
            "--lang",
            "rust",
            "--out-dir",
            str(bindings_dir),
            "--project-path",
            str(module_dir),
        ]
        result = run_cmd(gen_cmd, check=False, timeout=600.0)
        if result.returncode != 0:
            help_res = run_cmd([*cli, "generate", "--help"], check=False, timeout=60.0)
            help_text = help_res.stdout + help_res.stderr
            if "--project-path" not in help_text and "--module-path" in help_text:
                gen_cmd = [
                    *cli,
                    "generate",
                    "--lang",
                    "rust",
                    "--out-dir",
                    str(bindings_dir),
                    "--module-path",
                    str(module_dir),
                ]
                run_cmd(gen_cmd, timeout=600.0)
            else:
                raise RuntimeError(
                    f"Command failed: {' '.join(gen_cmd)}\n"
                    f"exit={result.returncode}\nSTDOUT:\n{result.stdout}\n"
                    f"STDERR:\n{result.stderr}"
                )

        run_cmd(["cargo", "build", "--release"], cwd=client_dir, timeout=1800.0)
        if not client_bin.exists():
            raise RuntimeError(
                f"Expected SpacetimeDB client binary not found at {client_bin}"
            )

    def _select_ports(self, cores: int) -> None:
        remote = self._remote_config()
        if remote is not None:
            self.port = remote.spacetimedb_port
        else:
            self.port = get_free_port(self.listen_ip)

    def prepare_client(self) -> None:
        # The tpcc-stdb-benchmark client binary runs on the client machine;
        # in remote mode start() runs on the server via SSH, so ensure the
        # local build happens here instead.
        self._resolve_cli()
        self._ensure_built()

    def start(self, cores: int) -> None:
        self._select_ports(cores)
        assert self.cfg.work_dir is not None
        self._ensure_work_dir()
        self.data_dir = Path(
            tempfile.mkdtemp(dir=self.cfg.work_dir, prefix="tpcc_stdb_")
        )
        self.log_dir = self.data_dir / "logs"
        self.log_dir.mkdir(parents=True, exist_ok=True)

        cli = self._resolve_cli()
        self._ensure_built()

        start_cmd = with_taskset(
            [
                *cli,
                "start",
                "--listen-addr",
                f"{self.listen_ip}:{self.port}",
                "--data-dir",
                str(self.data_dir / "data"),
                # Fail fast instead of prompting when something is wrong.
                "--non-interactive",
            ],
            build_core_mask(cores),
        )
        self._start_process(start_cmd, log_name="server.log")

        if not wait_for_port(self.listen_ip, self.port, timeout=60.0):
            raise RuntimeError("SpacetimeDB node failed to start (port not open)")
        # 1.12 has no reliable HTTP health endpoint (/health returns 404);
        # give the node a moment, then let the retried publish below act as
        # the readiness check.
        time.sleep(1.0)

        publish_cmd = [
            *cli,
            "publish",
            "-p",
            str(self._module_dir()),
            "--server",
            f"http://{self.listen_ip}:{self.port}",
            "--anonymous",
            "-y",
            # 1.12 treats `-c always` as a missing optional value followed by
            # a positional arg; the `=` form is required.
            "-c=always",
            "tpcc",
        ]
        last_err: Optional[Exception] = None
        for _attempt in range(3):
            try:
                run_cmd(publish_cmd, cwd=self.project_root, timeout=600.0)
                last_err = None
                break
            except Exception as e:
                last_err = e
                time.sleep(2.0)
        if last_err is not None:
            raise RuntimeError(f"SpacetimeDB publish failed: {last_err}")

    def stop(self) -> None:
        self.cleanup()

    def is_ready(self) -> bool:
        return self.port != 0 and wait_for_port(self.listen_ip, self.port, timeout=1.0)

    def connection_env(self) -> Dict[str, str]:
        return {}

    def benchmark_mode(self) -> str:
        return "spacetimedb-reducer"

    def extra_args(self) -> List[str]:
        return []

    def benchmark_command(
        self,
        project_root: Path,
        connections: int,
        seckill_hotspot_percent: int = 0,
        think_time_ms: int = 0,
    ) -> List[str]:
        client_bin = self._client_bin()
        if not client_bin.exists():
            raise RuntimeError(
                f"SpacetimeDB client binary not found at {client_bin}; "
                "it is built automatically on start()"
            )
        cfg = self.cfg
        cmd = [
            str(client_bin),
            "--uri",
            f"http://{self._host()}:{self.port}",
            "--module-name",
            "tpcc",
            "--warehouses",
            str(cfg.warehouses),
            "--districts-per-warehouse",
            str(cfg.districts),
            "--customers-per-district",
            str(cfg.customers),
            "--items",
            str(cfg.items),
            "--operation-count",
            str(cfg.operations),
            "--connection-count",
            str(connections),
            "--payment-percent",
            str(cfg.payment_percent),
            "--new-order-percent",
            str(cfg.new_order_percent),
        ]
        if cfg.workload != "tpcc":
            cmd.extend(
                [
                    "--workload",
                    cfg.workload,
                    "--seckill-items",
                    str(cfg.seckill_items),
                    "--seckill-payload-bytes",
                    str(cfg.seckill_payload_bytes),
                ]
            )
            if seckill_hotspot_percent > 0:
                cmd.extend(
                    ["--seckill-hotspot-percent", str(seckill_hotspot_percent)]
                )
        if cfg.hot_rows_per_warehouse > 0:
            cmd.extend(
                ["--hot-rows-per-warehouse", str(cfg.hot_rows_per_warehouse)]
            )
        if cfg.order_lines > 0:
            cmd.extend(["--order-lines", str(cfg.order_lines)])
        # Always passed (even 0): see the shared benchmark_command.
        cmd.extend(["--think-time-ms", str(think_time_ms)])
        return cmd


# ---------------------------------------------------------------------------
# Remote (two-machine) mode: SSH orchestration via paramiko
# ---------------------------------------------------------------------------


class ParamikoSsh:
    """Thin paramiko wrapper that drives the remote server host.

    paramiko is an optional dependency, imported lazily in connect(); only
    remote mode needs it. One instance drives one server run: connect(),
    put_config(), start_server_run(), then stop_server_run() / close().
    """

    def __init__(self, remote: RemoteConfig) -> None:
        self.remote = remote
        self._client: Any = None
        self._channel: Any = None
        self._output: List[str] = []

    def connect(self) -> None:
        try:
            import paramiko
        except ImportError:
            raise RuntimeError(
                "远程模式需要 paramiko 库 / remote mode requires the paramiko "
                "package; install it with: pip install --user paramiko"
            ) from None
        r = self.remote
        client = paramiko.SSHClient()
        client.load_system_host_keys()
        client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
        print(
            f"[note] ssh: unknown host keys for {r.server_host} are accepted "
            "automatically (paramiko.AutoAddPolicy)"
        )
        kwargs: Dict[str, Any] = {
            "hostname": r.server_host,
            "port": r.ssh_port,
            "username": r.ssh_user,
            # Default authentication order: explicit key file, then the SSH
            # agent and ~/.ssh keys, then the configured password.
            "allow_agent": True,
            "look_for_keys": True,
        }
        if r.ssh_key_filename:
            kwargs["key_filename"] = r.ssh_key_filename
        if r.ssh_password:
            kwargs["password"] = r.ssh_password
        try:
            client.connect(**kwargs)
        except Exception as e:
            raise RuntimeError(
                f"SSH connect to {r.ssh_user}@{r.server_host}:{r.ssh_port} "
                f"failed: {e}. 检查 ssh_user / server_host / ssh_port / "
                "ssh_key_filename 配置 (check the remote section settings)"
            ) from None
        self._client = client

    def put_config(self, content: str) -> str:
        """Write the effective yaml config onto the server host via SFTP."""
        assert self._client is not None
        remote_path = (
            f"{self.remote.server_project_root}/example/tpcc/bench_remote_agent.yaml"
        )
        sftp = self._client.open_sftp()
        try:
            with sftp.open(remote_path, "w") as f:
                f.write(content)
        finally:
            sftp.close()
        return remote_path

    def put_file(self, local_path: Path, remote_path: str) -> None:
        """Upload a local file to the server host via SFTP."""
        assert self._client is not None
        size_mib = local_path.stat().st_size / (1024 * 1024)
        print(
            f"[remote] uploading {local_path} -> "
            f"{self.remote.server_host}:{remote_path} ({size_mib:.1f} MiB)"
        )
        started = time.time()
        sftp = self._client.open_sftp()
        try:
            sftp.put(str(local_path), remote_path)
        finally:
            sftp.close()
        print(f"[remote] upload finished in {time.time() - started:.1f}s")

    def run_remote(self, command: str, timeout: float = 600.0) -> int:
        """Run a command on the server host through a login shell.

        `bash -l` puts tools the user added to PATH in ~/.profile (e.g.
        ~/.cargo/bin for cargo) on PATH, which a plain ssh exec channel
        would miss — same wrapping as start_server_run. Output is streamed
        to the local log with a [remote] prefix. Returns the exit code;
        raises RuntimeError on non-zero exit or timeout.
        """
        assert self._client is not None
        transport = self._client.get_transport()
        channel = transport.open_session()
        channel.set_combine_stderr(True)
        channel.exec_command(f"bash -l -c {shlex.quote(command)}")
        deadline = time.time() + timeout
        buf = b""
        try:
            while True:
                if channel.recv_ready():
                    data = channel.recv(4096)
                    if not data:
                        break
                    buf += data
                    while b"\n" in buf:
                        line, buf = buf.split(b"\n", 1)
                        print(
                            f"[remote] {line.decode('utf-8', errors='replace')}",
                            flush=True,
                        )
                    continue
                if channel.exit_status_ready():
                    while channel.recv_ready():
                        data = channel.recv(4096)
                        if not data:
                            break
                        buf += data
                    break
                if time.time() > deadline:
                    raise RuntimeError(
                        f"remote command timed out after {timeout}s: {command}"
                    )
                time.sleep(0.1)
            if buf:
                print(f"[remote] {buf.decode('utf-8', errors='replace')}", flush=True)
            status = channel.recv_exit_status()
        finally:
            channel.close()
        if status != 0:
            raise RuntimeError(f"remote command failed (exit {status}): {command}")
        return status

    def start_server_run(self, backend: str, cores: int) -> None:
        """Launch `bench_cross_db.py --server-run` on the server host.

        A daemon thread pumps the channel so the remote stdout/stderr show up
        in the local log (prefixed with [remote]) and early failures can be
        reported with their remote output.
        """
        assert self._client is not None
        root = self.remote.server_project_root
        # Run through a login shell: ssh exec channels use a non-interactive
        # non-login shell with a minimal PATH, so server binaries the user
        # added to PATH in ~/.profile (e.g. /usr/lib/postgresql/18/bin for
        # initdb/pg_ctl) would not be found otherwise.
        command = (
            f"cd {shlex.quote(root)}/example/tpcc && "
            "python3 bench_cross_db.py --config bench_remote_agent.yaml "
            f"--server-run {shlex.quote(backend)} --cores {int(cores)}"
        )
        transport = self._client.get_transport()
        channel = transport.open_session()
        channel.set_combine_stderr(True)
        self._output.clear()
        channel.exec_command(f"bash -l -c {shlex.quote(command)}")
        self._channel = channel
        threading.Thread(target=self._pump_channel, daemon=True).start()

    def _pump_channel(self) -> None:
        channel = self._channel
        while channel is not None:
            try:
                data = channel.recv(4096)
            except Exception:
                return
            if not data:
                return
            text = data.decode("utf-8", errors="replace")
            self._output.append(text)
            for line in text.splitlines():
                print(f"[remote] {line}", flush=True)

    def channel_exited(self) -> bool:
        return self._channel is not None and self._channel.exit_status_ready()

    def output_tail(self, max_chars: int = 4000) -> str:
        return "".join(self._output)[-max_chars:]

    def output_contains(self, marker: str) -> bool:
        return marker in "".join(self._output)

    def stop_server_run(self) -> None:
        """Ask the remote server-run process to stop, then close the channel.

        The remote side watches stdin: 'stop' triggers a clean backend stop.
        If it does not exit within 10s the channel is closed, which sends an
        EOF on the remote stdin and triggers the same watchdog cleanup.
        """
        channel = self._channel
        if channel is None:
            return
        try:
            channel.send("stop\n")
            deadline = time.time() + 10.0
            while time.time() < deadline and not channel.exit_status_ready():
                time.sleep(0.1)
        except Exception:
            pass
        finally:
            try:
                channel.close()
            except Exception:
                pass
            self._channel = None

    def close(self) -> None:
        self.stop_server_run()
        if self._client is not None:
            try:
                self._client.close()
            except Exception:
                pass
            self._client = None

    def remote_physical_cores(self) -> int:
        """Physical core count of the server host, parsed from lscpu."""
        assert self._client is not None
        _, stdout, _ = self._client.exec_command("lscpu -p=CPU,Core")
        text = stdout.read().decode("utf-8", errors="replace")
        cores = _physical_cores_from_lscpu(text)
        if cores is not None:
            return cores
        _, stdout, _ = self._client.exec_command("nproc")
        try:
            return max(1, int(stdout.read().decode("utf-8", errors="replace").strip()))
        except ValueError:
            raise RuntimeError(
                f"cannot detect physical cores on {self.remote.server_host}"
            ) from None


class RemoteBackend(BenchmarkBackend):
    """Decorator that runs a backend's server lifecycle on a remote host.

    start() opens a fresh SSH session, pushes the effective config and
    launches `bench_cross_db.py --server-run` on the server machine, then
    waits for the agent's [ready] marker (printed after the backend's own
    readiness check, e.g. after the spacetimedb module publish); stop()
    sends 'stop' over the channel.
    Every run gets a clean server instance, exactly like local mode. All
    client-facing methods delegate to the inner backend, whose _host()
    already resolves to the server host.
    """

    def __init__(
        self, inner: BenchmarkBackend, remote: RemoteConfig, config_text: str
    ) -> None:
        super().__init__(inner.project_root, inner.cfg)
        self.inner = inner
        self.remote = remote
        self.config_text = config_text
        self.name = inner.name
        self.mode = inner.mode
        self._ssh = ParamikoSsh(remote)

    @property
    def server_binary(self) -> str:
        return str(getattr(self.inner, "server_binary", ""))

    def start(self, cores: int) -> None:
        # Populate the inner backend's fixed remote ports without starting
        # anything locally; connection_env()/extra_args() read them.
        self.inner._select_ports(cores)
        self._ssh.connect()
        try:
            config_path = self._ssh.put_config(self.config_text)
            print(f"[remote] pushed config to {self.remote.server_host}:{config_path}")
            self._ssh.start_server_run(self.name, cores)
            # Build client-side artifacts (mudud mpk, spacetimedb client)
            # while the server comes up.
            self.inner.prepare_client()
            # Wait for the server agent's [ready] marker, not just the TCP
            # port: spacetimedb opens its port when the node starts, long
            # before the module publish makes the database usable, so a
            # port-only wait races the publish and the client sees 404s.
            # The generous deadline covers a cold first publish (the server
            # compiles the module to wasm on a fresh checkout).
            deadline = time.time() + 1800.0
            while time.time() < deadline:
                if self._ssh.output_contains("[ready]"):
                    return
                if self._ssh.channel_exited():
                    raise RuntimeError(
                        f"remote --server-run {self.name} exited before "
                        f"reporting readiness on {self.remote.server_host}:\n"
                        + self._ssh.output_tail()
                    )
                time.sleep(0.5)
            raise RuntimeError(
                f"remote {self.name} server did not report readiness on "
                f"{self.remote.server_host} within 1800s:\n"
                + self._ssh.output_tail()
            )
        except Exception:
            self._ssh.close()
            raise

    def stop(self) -> None:
        self._ssh.close()

    def is_ready(self) -> bool:
        port = self.inner.readiness_port()
        return port != 0 and wait_for_port(
            self.remote.server_host, port, timeout=1.0
        )

    def connection_env(self) -> Dict[str, str]:
        return self.inner.connection_env()

    def benchmark_mode(self) -> str:
        return self.inner.benchmark_mode()

    def extra_args(self) -> List[str]:
        return self.inner.extra_args()

    def benchmark_command(
        self,
        project_root: Path,
        connections: int,
        seckill_hotspot_percent: int = 0,
        think_time_ms: int = 0,
    ) -> List[str]:
        return self.inner.benchmark_command(
            project_root, connections, seckill_hotspot_percent, think_time_ms
        )


# Exclusions for build_sync_tarball: top-level directory names, directory
# names excluded at any depth, and exact relative directory prefixes.
_SYNC_EXCLUDE_TOP = {
    ".git",
    ".idea",
    ".vscode",
    "bench_cross_db_work",
    "bench_cross_db_results",
}
# Excluded at any depth: build output dirs exist below the root too (e.g.
# bindings/rs-shim/target, example/*/target) and are gigabytes each.
_SYNC_EXCLUDE_ANY_DEPTH = {"target", "__pycache__"}
# example/tpcc/spacetimedb must stay synced: in remote mode the server-side
# agent builds the module/client crates there (_ensure_built in start()).
_SYNC_EXCLUDE_PREFIXES: set = set()


def build_sync_tarball(project_root: Path, dest: Path) -> Path:
    """Pack the source tree (including uncommitted changes) into dest.

    Source only: build outputs, VCS/IDE metadata, benchmark work/result
    dirs and bulky logs are excluded. `.cargo/` is deliberately included —
    it carries the jobs limit and the cc-lld linker wrapper the remote
    build needs. Result is roughly 100 MB uncompressed / 20-30 MB gzipped.
    """

    def _excluded(rel: Path) -> bool:
        parts = rel.parts
        if not parts:
            return False
        if parts[0] in _SYNC_EXCLUDE_TOP:
            return True
        if any(part in _SYNC_EXCLUDE_ANY_DEPTH for part in parts):
            return True
        for prefix in _SYNC_EXCLUDE_PREFIXES:
            if parts[: len(prefix)] == prefix:
                return True
        return bool(fnmatch.fnmatch(rel.name, "nohup*.out*"))

    with tarfile.open(dest, "w:gz") as tar:
        for dirpath, dirnames, filenames in os.walk(project_root):
            rel_dir = Path(dirpath).relative_to(project_root)
            dirnames[:] = sorted(
                d for d in dirnames if not _excluded(rel_dir / d)
            )
            for name in sorted(filenames):
                rel = rel_dir / name
                if _excluded(rel):
                    continue
                tar.add(project_root / rel, arcname=str(rel), recursive=False)
    return dest


def setup_remote(cfg: BenchConfig) -> int:
    """Sync the local source tree to the server host and build release mudud.

    Full (non-incremental) tar.gz sync: an incremental sync would need
    rsync, which would break the paramiko-only convention. The remote cargo
    build needs crates.io access on the server host.
    """
    project_root = find_project_root()
    remote = RemoteConfig(cfg.remote)
    if not remote.server_project_root:
        print(
            "[error] remote.server_project_root is required for --setup-remote",
            file=sys.stderr,
        )
        return 1

    work_parent = project_root / "bench_cross_db_work"
    work_parent.mkdir(parents=True, exist_ok=True)
    tarball = work_parent / "mududb_sync.tar.gz"
    print(f"[info] packing source tree {project_root} ...")
    build_sync_tarball(project_root, tarball)
    size_mib = tarball.stat().st_size / (1024 * 1024)
    print(f"[info] tarball: {tarball} ({size_mib:.1f} MiB)")

    root = remote.server_project_root
    ssh = ParamikoSsh(remote)
    try:
        ssh.connect()
        remote_tar = "/tmp/mududb_sync.tar.gz"
        ssh.put_file(tarball, remote_tar)
        ssh.run_remote(
            f"mkdir -p {shlex.quote(root)} && "
            f"tar xzf {shlex.quote(remote_tar)} -C {shlex.quote(root)} && "
            f"rm -f {shlex.quote(remote_tar)}",
            timeout=600.0,
        )
        print(f"[info] building release mudud on {remote.server_host} ...")
        ssh.run_remote(
            f"cd {shlex.quote(root)} && cargo build --release -p mudud",
            timeout=3600.0,
        )
        ssh.run_remote(
            f"test -x {shlex.quote(root)}/target/release/mudud && "
            f"ls -l --time-style=long-iso {shlex.quote(root)}/target/release/mudud"
        )
    except RuntimeError as e:
        print(f"[error] setup-remote failed: {e}", file=sys.stderr)
        return 1
    finally:
        ssh.close()
    print(
        f"[info] remote setup complete: "
        f"{remote.server_host}:{root}/target/release/mudud"
    )
    return 0


# ---------------------------------------------------------------------------
# Benchmark runner
# ---------------------------------------------------------------------------


def build_benchmark_command(
    project_root: Path,
    backend: BenchmarkBackend,
    cfg: BenchConfig,
    connections: int,
    seckill_hotspot_percent: int = 0,
    think_time_ms: int = 0,
) -> List[str]:
    return backend.benchmark_command(
        project_root, connections, seckill_hotspot_percent, think_time_ms
    )


def run_single_benchmark(
    project_root: Path,
    backend: BenchmarkBackend,
    cfg: BenchConfig,
    cores: int,
    connections: int,
    run: int,
    client_cores: Optional[str],
    seckill_hotspot_percent: int = 0,
    think_time_ms: int = 0,
) -> TpccResult:
    backend.start(cores)
    if not backend.is_ready():
        raise RuntimeError(f"{backend.name} backend is not ready")

    env = os.environ.copy()
    env.update(backend.connection_env())
    # App installation on a debug server compiles the wasm module and can
    # take tens of seconds; the management client's default 10s timeout
    # would abort the install request before the server finishes.
    env.setdefault("MUDU_CLI_HTTP_TIMEOUT_SECS", "120")

    cmd = build_benchmark_command(
        project_root, backend, cfg, connections, seckill_hotspot_percent, think_time_ms
    )
    bench_cmd = with_taskset(cmd, client_cores)

    env_prefix = " ".join(f"{k}={shlex.quote(v)}" for k, v in env.items() if k.startswith("MUDU_"))
    shell_cmd = " ".join(shlex.quote(c) for c in bench_cmd)

    # Tee the benchmark output into a per-run log file next to the server
    # logs so a failed or timed-out run keeps its diagnostics on disk.
    log_parent = backend.log_dir or cfg.work_dir
    assert log_parent is not None
    log_parent.mkdir(parents=True, exist_ok=True)
    log_path = log_parent / f"bench_{backend.name}_{cores}c_{connections}n_run{run}.log"
    print(f"[bench] cd {shlex.quote(str(project_root))} && {env_prefix} {shell_cmd}")
    print(f"[bench] client log: {log_path}")

    # start_new_session puts the client (and its cargo-spawned children) in
    # their own process group so a timeout can kill the whole group instead
    # of leaving an orphaned benchmark holding database connections.
    # The async (stored-procedure) client keeps one tokio runtime plus one
    # socket per terminal, so it needs the same raised NOFILE limit as mudud;
    # otherwise runs with a few hundred connections die with EMFILE
    # ("Too many open files") on runtime creation / connect.
    proc = subprocess.Popen(
        bench_cmd,
        cwd=str(project_root),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
        start_new_session=True,
        preexec_fn=_raise_nofile_limit,
    )
    _log_child_nofile_limit(proc.pid, "bench client")
    try:
        stdout, stderr = proc.communicate(timeout=float(cfg.benchmark_timeout_secs))
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        stdout, stderr = proc.communicate()
        log_path.write_text(
            (stdout or "") + (stderr or ""), encoding="utf-8", errors="replace"
        )
        raise RuntimeError(
            f"Benchmark timed out after {cfg.benchmark_timeout_secs}s (client log: {log_path})"
        ) from None

    log_path.write_text(
        (stdout or "") + (stderr or ""), encoding="utf-8", errors="replace"
    )
    print(stdout)
    if stderr:
        print(stderr, file=sys.stderr)

    if proc.returncode != 0:
        raise RuntimeError(f"Benchmark exited with code {proc.returncode}")

    parsed = parse_benchmark_output(
        stdout,
        backend=backend.name,
        mode=backend.mode,
        cores=cores,
        connections=connections,
        run=run,
        cfg=cfg,
    )
    if parsed is None:
        raise RuntimeError("Could not parse benchmark summary from output")
    parsed.server_binary = getattr(backend, "server_binary", "")
    parsed.seckill_hotspot_percent = seckill_hotspot_percent
    parsed.think_time_ms = think_time_ms
    return parsed


# ---------------------------------------------------------------------------
# Aggregation and reporting
# ---------------------------------------------------------------------------


# Display labels for report outputs (summary table, CSV, plots). The raw
# backend/mode keys in results.json stay unchanged so existing result files
# remain mergeable; only the presentation layer uses these short names.
# `mududb-i` = interactive client over the sync adapter;
# `mududb-n` = near-data-processing (stored procedures run next to the data);
# `postgres-i` = PostgreSQL interactive; `postgres-n` = PostgreSQL
# near-data-processing (PL/pgSQL stored procedures).
DISPLAY_LABELS: Dict[Tuple[str, str], str] = {
    ("postgres", "sync"): "postgres-i",
    ("postgres-procedure", "pg-procedure"): "postgres-n",
    ("mysql", "sync"): "mysql",
    ("mududb-interactive-iouring", "sync"): "mududb-i",
    ("mududb-procedure-iouring", "tcp-multi-port"): "mududb-n",
    ("spacetimedb", "spacetimedb-reducer"): "spacetimedb",
}


def display_label(backend: str, mode: str) -> str:
    """Return the short report label for a (backend, mode) pair."""
    return DISPLAY_LABELS.get((backend, mode), f"{backend}/{mode}")


# Fixed (color, marker) per display label so every backend keeps the same
# visual identity in every chart, regardless of which backends a given run
# (or merged result set) happens to include.
LABEL_PLOT_STYLES: Dict[str, Tuple[str, str]] = {
    "postgres-i": ("#1f77b4", "o"),
    "postgres-n": ("#17becf", "P"),
    "mysql": ("#ff7f0e", "s"),
    "mududb-i": ("#2ca02c", "^"),
    "mududb-n": ("#d62728", "D"),
    "spacetimedb": ("#9467bd", "v"),
}

# Deterministic fallback pools for labels missing from LABEL_PLOT_STYLES:
# colors not used by the fixed table, then a distinct marker sequence.
_FALLBACK_PLOT_COLORS = [
    "#8c564b",
    "#e377c2",
    "#7f7f7f",
    "#bcbd22",
    "#17becf",
]
_FALLBACK_PLOT_MARKERS = ["P", "X", "*", "h", "<", ">", "p", "8"]


def resolve_plot_styles(labels: List[str]) -> Dict[str, Tuple[str, str]]:
    """Return label -> (color, marker), fixed for known labels and
    deterministic (sorted order) for unknown ones."""
    styles: Dict[str, Tuple[str, str]] = {}
    unknown: List[str] = []
    for label in labels:
        if label in LABEL_PLOT_STYLES:
            styles[label] = LABEL_PLOT_STYLES[label]
        else:
            unknown.append(label)
    used_colors = {color for color, _ in LABEL_PLOT_STYLES.values()}
    free_colors = [c for c in _FALLBACK_PLOT_COLORS if c not in used_colors]
    for idx, label in enumerate(sorted(unknown)):
        color = free_colors[idx % len(free_colors)]
        marker = _FALLBACK_PLOT_MARKERS[idx % len(_FALLBACK_PLOT_MARKERS)]
        styles[label] = (color, marker)
    return styles


def aggregate(results: List[TpccResult]) -> Dict[str, Any]:
    if not results:
        return {}
    n = len(results)

    def mean(field: str) -> float:
        return sum(getattr(r, field) for r in results) / n

    def std(field: str) -> float:
        vals = [getattr(r, field) for r in results]
        m = sum(vals) / n
        return (sum((v - m) ** 2 for v in vals) / n) ** 0.5

    return {
        "backend": results[0].backend,
        "mode": results[0].mode,
        "cores": results[0].cores,
        "connections": results[0].connections,
        "seckill_hotspot_percent": results[0].seckill_hotspot_percent,
        "think_time_ms": results[0].think_time_ms,
        "runs": n,
        "tps_mean": mean("tps"),
        "tps_std": std("tps"),
        "committed_tps_mean": mean("committed_tps"),
        "committed_tps_std": std("committed_tps"),
        "p99_mean": mean("p99_latency_ms"),
        "p99_std": std("p99_latency_ms"),
        "p50_mean": mean("p50_latency_ms"),
        "p90_mean": mean("p90_latency_ms"),
        "p999_mean": mean("p999_latency_ms"),
        "abort_rate_mean": mean("abort_rate_pct"),
        "throughput_mean": mean("throughput"),
    }


def save_results(
    results: List[TpccResult],
    aggregates: List[Dict[str, Any]],
    output_dir: Path,
) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)

    raw = [asdict(r) for r in results]
    (output_dir / "results.json").write_text(
        json.dumps(raw, indent=2), encoding="utf-8"
    )

    csv_path = output_dir / "summary.csv"
    if aggregates:
        # The first column carries the short display label; the raw
        # backend/mode columns are kept for machine processing.
        fieldnames = ["label", *aggregates[0].keys()]
        rows = [
            {"label": display_label(agg["backend"], agg["mode"]), **agg}
            for agg in aggregates
        ]
        with csv_path.open("w", newline="", encoding="utf-8") as f:
            writer = csv.DictWriter(f, fieldnames=fieldnames)
            writer.writeheader()
            writer.writerows(rows)

    print(f"[output] raw results: {output_dir / 'results.json'}")
    print(f"[output] summary csv: {csv_path}")


def print_summary_table(aggregates: List[Dict[str, Any]]) -> None:
    print("\n" + "=" * 124)
    print("TPC-C Cross-Database Summary")
    print("=" * 124)
    header = (
        f"{'Backend':<18} {'Cores':>5} {'Conn':>5} {'Runs':>4} "
        f"{'TPS':>10} {'TPS-std':>10} {'CommTPS':>10} {'CommTPS-std':>12} "
        f"{'P99(ms)':>10} {'P99-std':>10}"
    )
    print(header)
    print("-" * 124)
    for agg in aggregates:
        print(
            f"{display_label(agg['backend'], agg['mode']):<18} "
            f"{agg['cores']:>5} {agg['connections']:>5} {agg['runs']:>4} "
            f"{agg['tps_mean']:>10.2f} {agg['tps_std']:>10.2f} "
            f"{agg['committed_tps_mean']:>10.2f} {agg['committed_tps_std']:>12.2f} "
            f"{agg['p99_mean']:>10.3f} {agg['p99_std']:>10.3f}"
        )
    print("=" * 124)


# ---------------------------------------------------------------------------
# Plotting
# ---------------------------------------------------------------------------


def plot_results(
    aggregates: List[Dict[str, Any]],
    output_dir: Path,
) -> None:
    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError as e:
        print(f"[warn] matplotlib not available, skipping plots: {e}")
        return

    output_dir.mkdir(parents=True, exist_ok=True)

    # Build data keyed by backend-mode display label.
    backends: Dict[str, List[Dict[str, Any]]] = {}
    for agg in aggregates:
        label = display_label(agg["backend"], agg["mode"])
        backends.setdefault(label, []).append(agg)
    plot_styles = resolve_plot_styles(list(backends))

    core_counts = sorted(set(a["cores"] for a in aggregates))
    conn_counts = sorted(set(a["connections"] for a in aggregates))
    hotspot_counts = sorted(set(a["seckill_hotspot_percent"] for a in aggregates))
    think_time_counts = sorted(set(a["think_time_ms"] for a in aggregates))

    # Subplot grid over distinct core counts (charts 1/2/5/6).
    n_cores = len(core_counts)
    cols = min(3, max(1, n_cores))
    rows = (n_cores + cols - 1) // cols
    # Subplot grid over distinct connection counts (charts 3/4).
    n_conns = len(conn_counts)
    cols2 = min(3, max(1, n_conns))
    rows2 = (n_conns + cols2 - 1) // cols2

    # Chart 1: TPS vs Connections (subplot per core count)
    if len(conn_counts) < 2:
        print("[plot] skipping tps_vs_connections.png: <2 distinct connections values")
    else:
        fig, axes = plt.subplots(rows, cols, figsize=(6 * cols, 5 * rows), squeeze=False)
        for idx, cores in enumerate(core_counts):
            ax = axes[idx // cols][idx % cols]
            for label, items in backends.items():
                points = sorted(
                    [a for a in items if a["cores"] == cores],
                    key=lambda x: x["connections"],
                )
                if not points:
                    continue
                conns = [p["connections"] for p in points]
                tps = [p["tps_mean"] for p in points]
                color, marker = plot_styles[label]
                ax.plot(conns, tps, color=color, marker=marker, label=label)
            ax.set_xlabel("Connections")
            ax.set_ylabel("TPS")
            ax.set_title(f"{cores} core{'s' if cores > 1 else ''}")
            ax.set_xscale("log", base=2)
            ax.grid(True, linestyle="--", alpha=0.5)
            ax.legend(fontsize=7)
        for idx in range(n_cores, rows * cols):
            fig.delaxes(axes[idx // cols][idx % cols])
        fig.suptitle("TPC-C TPS vs Connections")
        plt.tight_layout()
        plt.savefig(output_dir / "tps_vs_connections.png", dpi=150)
        plt.close()
        print(f"[plot] saved {output_dir / 'tps_vs_connections.png'}")

    # Chart 2: P99 Latency vs Connections
    if len(conn_counts) < 2:
        print("[plot] skipping p99_vs_connections.png: <2 distinct connections values")
    else:
        fig, axes = plt.subplots(rows, cols, figsize=(6 * cols, 5 * rows), squeeze=False)
        for idx, cores in enumerate(core_counts):
            ax = axes[idx // cols][idx % cols]
            for label, items in backends.items():
                points = sorted(
                    [a for a in items if a["cores"] == cores],
                    key=lambda x: x["connections"],
                )
                if not points:
                    continue
                conns = [p["connections"] for p in points]
                p99 = [p["p99_mean"] for p in points]
                color, marker = plot_styles[label]
                ax.plot(conns, p99, color=color, marker=marker, label=label)
            ax.set_xlabel("Connections")
            ax.set_ylabel("P99 Latency (ms)")
            ax.set_title(f"{cores} core{'s' if cores > 1 else ''}")
            ax.set_xscale("log", base=2)
            ax.set_yscale("log")
            ax.grid(True, linestyle="--", alpha=0.5)
            ax.legend(fontsize=7)
        for idx in range(n_cores, rows * cols):
            fig.delaxes(axes[idx // cols][idx % cols])
        fig.suptitle("TPC-C P99 Latency vs Connections")
        plt.tight_layout()
        plt.savefig(output_dir / "p99_vs_connections.png", dpi=150)
        plt.close()
        print(f"[plot] saved {output_dir / 'p99_vs_connections.png'}")

    # Chart 3: TPS vs Cores (subplot per connection count)
    if len(core_counts) < 2:
        print("[plot] skipping tps_vs_cores.png: <2 distinct cores values")
    else:
        fig, axes = plt.subplots(rows2, cols2, figsize=(6 * cols2, 5 * rows2), squeeze=False)
        for idx, conns in enumerate(conn_counts):
            ax = axes[idx // cols2][idx % cols2]
            for label, items in backends.items():
                points = sorted(
                    [a for a in items if a["connections"] == conns],
                    key=lambda x: x["cores"],
                )
                if not points:
                    continue
                cores = [p["cores"] for p in points]
                tps = [p["tps_mean"] for p in points]
                color, marker = plot_styles[label]
                ax.plot(cores, tps, color=color, marker=marker, label=label)
            ax.set_xlabel("CPU Cores")
            ax.set_ylabel("TPS")
            ax.set_title(f"{conns} connection{'s' if conns > 1 else ''}")
            ax.grid(True, linestyle="--", alpha=0.5)
            ax.legend(fontsize=7)
        for idx in range(n_conns, rows2 * cols2):
            fig.delaxes(axes[idx // cols2][idx % cols2])
        fig.suptitle("TPC-C TPS vs CPU Cores")
        plt.tight_layout()
        plt.savefig(output_dir / "tps_vs_cores.png", dpi=150)
        plt.close()
        print(f"[plot] saved {output_dir / 'tps_vs_cores.png'}")

    # Chart 4: P99 Latency vs Cores (subplot per connection count)
    if len(core_counts) < 2:
        print("[plot] skipping p99_vs_cores.png: <2 distinct cores values")
    else:
        fig, axes = plt.subplots(rows2, cols2, figsize=(6 * cols2, 5 * rows2), squeeze=False)
        for idx, conns in enumerate(conn_counts):
            ax = axes[idx // cols2][idx % cols2]
            for label, items in backends.items():
                points = sorted(
                    [a for a in items if a["connections"] == conns],
                    key=lambda x: x["cores"],
                )
                if not points:
                    continue
                cores = [p["cores"] for p in points]
                p99 = [p["p99_mean"] for p in points]
                color, marker = plot_styles[label]
                ax.plot(cores, p99, color=color, marker=marker, label=label)
            ax.set_xlabel("CPU Cores")
            ax.set_ylabel("P99 Latency (ms)")
            ax.set_title(f"{conns} connection{'s' if conns > 1 else ''}")
            ax.set_yscale("log")
            ax.grid(True, linestyle="--", alpha=0.5)
            ax.legend(fontsize=7)
        for idx in range(n_conns, rows2 * cols2):
            fig.delaxes(axes[idx // cols2][idx % cols2])
        fig.suptitle("TPC-C P99 Latency vs CPU Cores")
        plt.tight_layout()
        plt.savefig(output_dir / "p99_vs_cores.png", dpi=150)
        plt.close()
        print(f"[plot] saved {output_dir / 'p99_vs_cores.png'}")

    # Chart 5: TPS vs Seckill Hotspot Percent (subplot per core count)
    if len(hotspot_counts) < 2:
        print("[plot] skipping tps_vs_hotspot.png: <2 distinct hotspot values")
    else:
        fig, axes = plt.subplots(rows, cols, figsize=(6 * cols, 5 * rows), squeeze=False)
        for idx, cores in enumerate(core_counts):
            ax = axes[idx // cols][idx % cols]
            for label, items in backends.items():
                points = sorted(
                    [a for a in items if a["cores"] == cores],
                    key=lambda x: x["seckill_hotspot_percent"],
                )
                if not points:
                    continue
                hotspots = [p["seckill_hotspot_percent"] for p in points]
                tps = [p["tps_mean"] for p in points]
                color, marker = plot_styles[label]
                ax.plot(hotspots, tps, color=color, marker=marker, label=label)
            ax.set_xlabel("Seckill Hotspot Percent")
            ax.set_ylabel("TPS")
            ax.set_title(f"{cores} core{'s' if cores > 1 else ''}")
            ax.grid(True, linestyle="--", alpha=0.5)
            ax.legend(fontsize=7)
        for idx in range(n_cores, rows * cols):
            fig.delaxes(axes[idx // cols][idx % cols])
        fig.suptitle("TPC-C TPS vs Seckill Hotspot Percent")
        plt.tight_layout()
        plt.savefig(output_dir / "tps_vs_hotspot.png", dpi=150)
        plt.close()
        print(f"[plot] saved {output_dir / 'tps_vs_hotspot.png'}")

    # Chart 6: P99 Latency vs Seckill Hotspot Percent (subplot per core count)
    if len(hotspot_counts) < 2:
        print("[plot] skipping p99_vs_hotspot.png: <2 distinct hotspot values")
    else:
        fig, axes = plt.subplots(rows, cols, figsize=(6 * cols, 5 * rows), squeeze=False)
        for idx, cores in enumerate(core_counts):
            ax = axes[idx // cols][idx % cols]
            for label, items in backends.items():
                points = sorted(
                    [a for a in items if a["cores"] == cores],
                    key=lambda x: x["seckill_hotspot_percent"],
                )
                if not points:
                    continue
                hotspots = [p["seckill_hotspot_percent"] for p in points]
                p99 = [p["p99_mean"] for p in points]
                color, marker = plot_styles[label]
                ax.plot(hotspots, p99, color=color, marker=marker, label=label)
            ax.set_xlabel("Seckill Hotspot Percent")
            ax.set_ylabel("P99 Latency (ms)")
            ax.set_title(f"{cores} core{'s' if cores > 1 else ''}")
            ax.set_yscale("log")
            ax.grid(True, linestyle="--", alpha=0.5)
            ax.legend(fontsize=7)
        for idx in range(n_cores, rows * cols):
            fig.delaxes(axes[idx // cols][idx % cols])
        fig.suptitle("TPC-C P99 Latency vs Seckill Hotspot Percent")
        plt.tight_layout()
        plt.savefig(output_dir / "p99_vs_hotspot.png", dpi=150)
        plt.close()
        print(f"[plot] saved {output_dir / 'p99_vs_hotspot.png'}")

    # Chart 7: TPS vs Think Time (subplot per core count)
    if len(think_time_counts) < 2:
        print("[plot] skipping tps_vs_think_time.png: <2 distinct think-time values")
    else:
        fig, axes = plt.subplots(rows, cols, figsize=(6 * cols, 5 * rows), squeeze=False)
        for idx, cores in enumerate(core_counts):
            ax = axes[idx // cols][idx % cols]
            for label, items in backends.items():
                points = sorted(
                    [a for a in items if a["cores"] == cores],
                    key=lambda x: x["think_time_ms"],
                )
                if not points:
                    continue
                think_times = [p["think_time_ms"] for p in points]
                tps = [p["tps_mean"] for p in points]
                color, marker = plot_styles[label]
                ax.plot(think_times, tps, color=color, marker=marker, label=label)
            ax.set_xlabel("Think Time (ms)")
            ax.set_ylabel("TPS")
            ax.set_title(f"{cores} core{'s' if cores > 1 else ''}")
            ax.grid(True, linestyle="--", alpha=0.5)
            ax.legend(fontsize=7)
        for idx in range(n_cores, rows * cols):
            fig.delaxes(axes[idx // cols][idx % cols])
        fig.suptitle("TPC-C TPS vs Think Time")
        plt.tight_layout()
        plt.savefig(output_dir / "tps_vs_think_time.png", dpi=150)
        plt.close()
        print(f"[plot] saved {output_dir / 'tps_vs_think_time.png'}")

    # Chart 8: P99 Latency vs Think Time (subplot per core count)
    if len(think_time_counts) < 2:
        print("[plot] skipping p99_vs_think_time.png: <2 distinct think-time values")
    else:
        fig, axes = plt.subplots(rows, cols, figsize=(6 * cols, 5 * rows), squeeze=False)
        for idx, cores in enumerate(core_counts):
            ax = axes[idx // cols][idx % cols]
            for label, items in backends.items():
                points = sorted(
                    [a for a in items if a["cores"] == cores],
                    key=lambda x: x["think_time_ms"],
                )
                if not points:
                    continue
                think_times = [p["think_time_ms"] for p in points]
                p99 = [p["p99_mean"] for p in points]
                color, marker = plot_styles[label]
                ax.plot(think_times, p99, color=color, marker=marker, label=label)
            ax.set_xlabel("Think Time (ms)")
            ax.set_ylabel("P99 Latency (ms)")
            ax.set_title(f"{cores} core{'s' if cores > 1 else ''}")
            ax.set_yscale("log")
            ax.grid(True, linestyle="--", alpha=0.5)
            ax.legend(fontsize=7)
        for idx in range(n_cores, rows * cols):
            fig.delaxes(axes[idx // cols][idx % cols])
        fig.suptitle("TPC-C P99 Latency vs Think Time")
        plt.tight_layout()
        plt.savefig(output_dir / "p99_vs_think_time.png", dpi=150)
        plt.close()
        print(f"[plot] saved {output_dir / 'p99_vs_think_time.png'}")


# ---------------------------------------------------------------------------
# Documentation
# ---------------------------------------------------------------------------


def write_documentation(output_dir: Path) -> None:
    """Copy the companion Markdown documentation into the output directory."""
    doc_path = output_dir / "bench_cross_db.md"
    static_doc = Path(__file__).with_suffix(".md")
    if static_doc.exists():
        shutil.copy2(static_doc, doc_path)
    else:
        # Fallback minimal doc if the static file is missing.
        doc_path.write_text(
            "# TPC-C Cross-Database Benchmark\n\n"
            "See `bench_cross_db.py` for usage information.\n",
            encoding="utf-8",
        )
    print(f"[doc] wrote {doc_path}")


# ---------------------------------------------------------------------------
# CLI and main
# ---------------------------------------------------------------------------


def make_default_config(physical_cores: int) -> Dict[str, Any]:
    return {
        "defaults": {
            "backends": ["postgres", "postgres-procedure", "mysql", "mududb"],
            "cpu_cores": [1, 2, 4, min(8, physical_cores), physical_cores],
            "connections": [1, 2, 4, 8, 16, 32, 64],
            "repeats": 3,
            "warmup_operations": 0,
            "perf_sample_rate": 0,
            "warehouses": 4,
            "districts": 10,
            "customers": 100,
            "items": 100,
            "operations": 1000,
            "payment_percent": 50,
            "new_order_percent": 35,
            "postgres": {
                "initdb": "initdb",
                "pg_ctl": "pg_ctl",
                "psql": "psql",
                "user": "postgres",
                "password": "postgres",
                "database": "tpcc",
            },
            "mysql": {
                "mysqld": "mysqld",
                "mysqladmin": "mysqladmin",
                "mysql": "mysql",
                "user": "root",
                "password": "",
                "database": "tpcc",
            },
            "mudud": {
                "listen_ip": "127.0.0.1",
                "ring_entries": 1024,
                "modes": {
                    "interactive": {
                        "server_mode": "iouring",
                        "interactive_mode": "interactive",
                    },
                    "procedure": {
                        "server_mode": "iouring",
                        "interactive_mode": "procedure",
                    },
                    "procedure-iouring": {
                        "server_mode": "iouring",
                        "interactive_mode": "procedure",
                    },
                },
            },
            "spacetimedb": {
                "version": "1.12.0",
                "cli_path": "",
                "install_dir": "",
                "listen_ip": "127.0.0.1",
            },
            "client_cpu_cores": None,
            "client_cpu_offset": 0,
            "output_dir": "./bench_cross_db_results",
            "keep_data": False,
            "buffer_pool_ratio": 0.8,
        },
        "tests": [
            {
                "name": "tpcc-baseline",
            },
        ],
    }


DEFAULT_BACKENDS = [
    "postgres",
    "postgres-procedure",
    "mysql",
    "mududb",
]


class MududMode(NamedTuple):
    """One MuduDB benchmark mode entry from the `mudud.modes` config mapping.

    Each entry pairs a server mode (how mudud drives I/O) with an
    interactive mode (how the benchmark client issues transactions).
    """

    name: str  # entry name, e.g. "procedure-iouring"
    interactive_mode: str  # "interactive" or "procedure"
    server_mode: str  # "tokio", "iouring", ...

    @property
    def backend_key(self) -> str:
        return f"mududb-{self.name}"


# The default MuduDB mode entries, used when the config has no `mudud.modes`
# mapping. Mirrors the historical fixed triple.
DEFAULT_MUDUDB_MODES: List[MududMode] = [
    MududMode("interactive", "interactive", "iouring"),
    MududMode("procedure", "procedure", "iouring"),
    MududMode("procedure-iouring", "procedure", "iouring"),
]

# Coarse DB-type names accepted in the config file and on the CLI. Each
# expands to one or more concrete backend keys (see build_backends); the
# `mududb` expansion is controlled by the `mudud.modes` config mapping.
DB_TYPE_BACKENDS: Dict[str, List[str]] = {
    "postgres": ["postgres"],
    "postgresql": ["postgres"],
    "postgres-procedure": ["postgres-procedure"],
    "mysql": ["mysql"],
    "spacetimedb": ["spacetimedb"],
}

# Concrete backend keys not derived from `mudud.modes`; the MuduDB keys
# (`mududb-<mode name>`) are generated from the config's mode entries.
CONCRETE_BACKENDS = [
    "postgres",
    "postgres-procedure",
    "mysql",
    "spacetimedb",
]

_MUDUD_MODE_NAME_RE = re.compile(r"^[a-z0-9][a-z0-9-]*$")

# Values accepted by the `server_mode` sub-option (see the mode_map in
# MududbBackend._write_config) and by `interactive_mode`.
_MUDUD_SERVER_MODES = ("tokio", "iouring", "io_uring", "legacy")
_MUDUD_INTERACTIVE_MODES = ("interactive", "procedure")


def parse_mudud_modes(mudud_cfg: Dict[str, Any]) -> List[MududMode]:
    """Read the MuduDB mode entries from the `mudud.modes` config mapping.

    Each entry is `<name>: {server_mode, interactive_mode}` and becomes a
    concrete backend key `mududb-<name>`. Defaults to the three standard
    modes when `mudud.modes` is absent.
    """
    raw = mudud_cfg.get("modes")
    if raw is None:
        return list(DEFAULT_MUDUDB_MODES)
    if isinstance(raw, list):
        raise ValueError(
            "mudud.modes is now a mapping of mode name to sub-options, not "
            "a list. Migrate e.g. `modes: [interactive]` to:\n"
            "  modes:\n"
            "    interactive:\n"
            "      server_mode: iouring\n"
            "      interactive_mode: interactive"
        )
    if not isinstance(raw, dict):
        raise ValueError(
            "mudud.modes must be a mapping of mode name to sub-options, "
            f"got {type(raw).__name__}"
        )
    modes: List[MududMode] = []
    for name, sub in raw.items():
        key = str(name).strip().lower()
        if not _MUDUD_MODE_NAME_RE.match(key):
            raise ValueError(
                f"Invalid mudud mode name '{name}'; use lowercase letters, "
                "digits and '-' (e.g. 'procedure-iouring')"
            )
        if not isinstance(sub, dict):
            raise ValueError(
                f"mudud.modes.{key} must be a mapping with "
                "'interactive_mode' and optional 'server_mode'"
            )
        interactive_mode = str(sub.get("interactive_mode", "")).strip().lower()
        if interactive_mode not in _MUDUD_INTERACTIVE_MODES:
            raise ValueError(
                f"mudud.modes.{key}.interactive_mode must be one of "
                f"{list(_MUDUD_INTERACTIVE_MODES)}, got "
                f"'{sub.get('interactive_mode')}'"
            )
        server_mode = str(sub.get("server_mode", "iouring")).strip().lower()
        if server_mode not in _MUDUD_SERVER_MODES:
            raise ValueError(
                f"mudud.modes.{key}.server_mode must be one of "
                f"{list(_MUDUD_SERVER_MODES)}, got "
                f"'{sub.get('server_mode')}'"
            )
        modes.append(MududMode(key, interactive_mode, server_mode))
    if not modes:
        raise ValueError("mudud.modes is empty; define at least one mode entry")
    return modes


def resolve_backends(
    requested: List[str], mududb_modes: Optional[List[MududMode]] = None
) -> List[str]:
    """Expand DB-type names into concrete backend keys.

    Accepts both coarse DB types (``postgres``/``postgresql``, ``mysql``,
    ``mududb``, ``spacetimedb``) and the concrete backend keys themselves.
    ``mududb`` expands to the ``mududb-<name>`` keys of ``mududb_modes``
    (from the `mudud.modes` config mapping), defaulting to the three
    standard MuduDB modes.
    """
    if mududb_modes is None:
        mududb_modes = list(DEFAULT_MUDUDB_MODES)
    mududb_keys = [m.backend_key for m in mududb_modes]
    concrete = CONCRETE_BACKENDS + mududb_keys
    enabled: List[str] = []
    for entry in requested:
        key = entry.strip().lower()
        if not key:
            continue
        if key == "mududb":
            enabled.extend(mududb_keys)
        elif key in DB_TYPE_BACKENDS:
            enabled.extend(DB_TYPE_BACKENDS[key])
        elif key in concrete:
            enabled.append(key)
        else:
            raise ValueError(
                f"Unknown backend '{entry}'. "
                f"DB types: {['postgres', 'postgresql', 'postgres-procedure', 'mysql', 'mududb', 'spacetimedb']}; "
                f"concrete backends: {concrete}"
            )
    # Deduplicate while preserving order (e.g. 'postgres' plus
    # 'postgresql' would otherwise run postgres twice).
    seen = set()
    unique = []
    for key in enabled:
        if key not in seen:
            seen.add(key)
            unique.append(key)
    return unique


def build_backends(
    project_root: Path,
    cfg: BenchConfig,
    enabled: List[str],
    mudud_modes: Optional[List[MududMode]] = None,
    remote_config_text: Optional[str] = None,
) -> List[BenchmarkBackend]:
    if mudud_modes is None:
        mudud_modes = list(DEFAULT_MUDUDB_MODES)
    backends: List[BenchmarkBackend] = []
    mapping: Dict[str, BenchmarkBackend] = {
        "postgres": PostgresBackend(project_root, cfg),
        "postgres-procedure": PostgresProcedureBackend(project_root, cfg),
        "mysql": MySqlBackend(project_root, cfg),
        "spacetimedb": SpacetimeDbBackend(project_root, cfg),
    }
    for m in mudud_modes:
        mapping[m.backend_key] = MududbBackend(
            project_root,
            cfg,
            m.interactive_mode,
            server_mode=m.server_mode,
            name=m.backend_key,
        )
    for key in enabled:
        if key not in mapping:
            raise ValueError(f"Unknown backend '{key}'. Choose from {list(mapping)}")
        backends.append(mapping[key])
    # Remote mode (client side only): wrap each backend so its server
    # lifecycle runs on the server host over SSH. The --server-run agent on
    # the server passes remote_config_text=None and gets the raw backends.
    if remote_config_text is not None and remote_enabled(cfg):
        remote = RemoteConfig(cfg.remote)
        if not remote.server_project_root:
            raise ValueError(
                "remote.server_project_root is required when "
                "remote.server_host is set"
            )
        backends = [
            RemoteBackend(b, remote, remote_config_text) for b in backends
        ]
    return backends


def server_run_main(args: argparse.Namespace) -> int:
    """Entry point for --server-run: run a single backend on this machine.

    Used by the remote (two-machine) mode: the client machine SSHes in and
    runs this command, then drives the lifecycle over stdin. After the
    backend is up, a `[ready]` line is printed and stdin is read until a
    'stop' line or EOF arrives (EOF is the watchdog for a dropped SSH
    connection); SIGTERM stops it too. The backend is always stopped before
    exiting.
    """
    if not args.config.exists():
        print(f"[error] config file not found: {args.config}", file=sys.stderr)
        return 1
    cores = args.cores
    if cores is None or cores < 1:
        print(
            "[error] --server-run requires --cores N (N >= 1)", file=sys.stderr
        )
        return 1

    cfg_dict = load_config(args.config)
    cfg = BenchConfig.from_dict(cfg_dict)
    project_root = find_project_root()

    # Same work-dir rules as main(): a fresh directory per server run.
    if cfg.work_dir is None:
        work_parent = project_root / "bench_cross_db_work"
        work_parent.mkdir(parents=True, exist_ok=True)
        cfg.work_dir = Path(tempfile.mkdtemp(prefix="server_run_", dir=work_parent))
    cfg.work_dir = cfg.work_dir.resolve()
    cfg.work_dir.mkdir(parents=True, exist_ok=True)

    try:
        mududb_modes = parse_mudud_modes(cfg_dict.get("mudud", {}) or {})
        enabled = resolve_backends([args.server_run], mududb_modes=mududb_modes)
    except ValueError as e:
        print(f"[error] {e}", file=sys.stderr)
        return 1
    if len(enabled) != 1:
        print(
            f"[error] --server-run needs exactly one concrete backend, "
            f"got: {', '.join(enabled)}",
            file=sys.stderr,
        )
        return 1
    backend = build_backends(
        project_root, cfg, enabled, mudud_modes=mududb_modes
    )[0]
    print(
        f"[info] server-run: backend={backend.name} cores={cores} "
        f"work_dir={cfg.work_dir}",
        flush=True,
    )

    class _StopRequested(Exception):
        pass

    def _handle_sigterm(signum: int, frame: Any) -> None:
        # Raising is deliberate: PEP 475 would otherwise restart the blocked
        # stdin read after the handler returns.
        raise _StopRequested()

    signal.signal(signal.SIGTERM, _handle_sigterm)
    exit_code = 0
    try:
        backend.start(cores)
        if not backend.is_ready():
            raise RuntimeError(f"{backend.name} backend is not ready")
        print(
            f"[ready] {backend.name} (cores={cores}); "
            "send 'stop' on stdin to shut down",
            flush=True,
        )
        try:
            for line in sys.stdin:
                if line.strip().lower() == "stop":
                    break
        except _StopRequested:
            print("[info] SIGTERM received; shutting down", flush=True)
    except Exception as e:
        print(f"[error] server-run failed: {e}", file=sys.stderr)
        exit_code = 1
    finally:
        try:
            backend.stop()
        except Exception as e:
            print(f"[warn] backend stop failed: {e}", file=sys.stderr)
        if not cfg.keep_data and cfg.work_dir is not None and cfg.work_dir.exists():
            shutil.rmtree(cfg.work_dir, ignore_errors=True)
    if exit_code == 0:
        print("[info] server-run exited cleanly", flush=True)
    return exit_code


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run TPC-C cross-database benchmark",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "--config",
        type=Path,
        default=Path("bench_cross_db_config.yaml"),
        help="Path to YAML/JSON configuration file",
    )
    parser.add_argument(
        "--backends",
        type=str,
        default=None,
        help=(
            "Comma-separated list of backends to run. Accepts DB types "
            "(postgres, postgres-procedure, mysql, mududb, spacetimedb) and "
            "concrete backend keys "
            "(mududb-<mode>, one per entry in the config's mudud.modes "
            "mapping). The mududb type expands to all of those entries. "
            "Overrides the config file's `backends` list; defaults to "
            + ",".join(DEFAULT_BACKENDS)
            + ". SpacetimeDB is opt-in: name it explicitly to include it."
        ),
    )
    parser.add_argument(
        "--write-default-config",
        type=Path,
        default=None,
        help="Write a default config file and exit",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=None,
        help="Override output directory",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help=(
            "Expand the config and print each test's backends and full "
            "sweep matrix (cores x connections x hotspot x repeats), then "
            "exit without touching remote hosts or starting any servers"
        ),
    )
    parser.add_argument(
        "--server-run",
        metavar="BACKEND",
        default=None,
        help=(
            "Server-agent mode for the two-machine (remote) mode: start only "
            "BACKEND on this machine, print [ready], then block until 'stop' "
            "or EOF arrives on stdin. Normally launched over SSH by the "
            "client machine; see bench_cross_db.md."
        ),
    )
    parser.add_argument(
        "--cores",
        type=int,
        default=None,
        help="CPU core count for --server-run",
    )
    parser.add_argument(
        "--setup-remote",
        action="store_true",
        help=(
            "Sync the local source tree to the remote server host and build "
            "the release mudud binary there, then exit. Requires a remote "
            "section in the config; mutually exclusive with --server-run."
        ),
    )
    args = parser.parse_args()

    if args.server_run is not None and args.setup_remote:
        parser.error("--setup-remote and --server-run are mutually exclusive")
    if args.server_run is not None:
        return server_run_main(args)

    physical_cores = detect_physical_cores()
    print(f"[info] detected {physical_cores} physical CPU core(s)")

    if args.write_default_config is not None:
        cfg_dict = make_default_config(physical_cores)
        args.write_default_config.write_text(
            json.dumps(cfg_dict, indent=2), encoding="utf-8"
        )
        print(f"[info] wrote default config to {args.write_default_config}")
        return 0

    if not args.config.exists():
        print(
            f"[error] config file not found: {args.config}. "
            f"Generate one with --write-default-config {args.config}",
            file=sys.stderr,
        )
        return 1

    cfg_dict = load_config(args.config)
    try:
        tests = expand_config_tests(cfg_dict)
    except ValueError as e:
        print(f"[error] {e}", file=sys.stderr)
        return 1

    # Infrastructure settings (backend binaries/lifecycle, remote, output and
    # work dirs, client pinning, buffer pool) are global and live in
    # `defaults`; tests may only override sweep/workload parameters.
    defaults = cfg_dict.get("defaults") or {}
    cfg = BenchConfig.from_dict(defaults)
    if args.output_dir is not None:
        cfg.output_dir = args.output_dir

    # Backend selection per test: CLI --backends wins, then the test's
    # `backends` list, then the built-in default. The `mududb` DB type
    # expands to the mode entries in the config's `mudud.modes` mapping.
    try:
        mududb_modes = parse_mudud_modes(defaults.get("mudud", {}) or {})
        test_plans: List[Tuple[str, Dict[str, Any], List[str]]] = []
        for test_name, test_dict in tests:
            if args.backends is not None:
                requested = [b.strip() for b in args.backends.split(",") if b.strip()]
            elif test_dict.get("backends"):
                requested = [str(b).strip() for b in test_dict["backends"]]
            else:
                requested = list(DEFAULT_BACKENDS)
            enabled = resolve_backends(requested, mududb_modes=mududb_modes)
            test_plans.append((test_name, test_dict, enabled))
    except ValueError as e:
        print(f"[error] {e}", file=sys.stderr)
        return 1

    if args.dry_run:
        for test_name, test_dict, enabled in test_plans:
            test_cfg = BenchConfig.from_dict(test_dict)
            n_runs = (
                len(enabled)
                * len(test_cfg.cpu_cores)
                * len(test_cfg.connections)
                * len(test_cfg.seckill_hotspot_percents)
                * len(test_cfg.think_times_ms)
                * test_cfg.repeats
            )
            print(f"== test: {test_name} ==")
            print(f"  backends: {', '.join(enabled)}")
            print(f"  workload: {test_cfg.workload}")
            print(f"  cores: {test_cfg.cpu_cores}")
            print(f"  connections: {test_cfg.connections}")
            print(f"  seckill_hotspot_percents: {test_cfg.seckill_hotspot_percents}")
            print(f"  think_times_ms: {test_cfg.think_times_ms}")
            print(f"  repeats: {test_cfg.repeats}")
            print(
                f"  matrix: {len(enabled)} backend(s) x "
                f"{len(test_cfg.cpu_cores)} cores x "
                f"{len(test_cfg.connections)} connections x "
                f"{len(test_cfg.seckill_hotspot_percents)} hotspot(s) x "
                f"{len(test_cfg.think_times_ms)} think-time(s) x "
                f"{test_cfg.repeats} repeats = {n_runs} run(s)"
            )
        return 0

    if args.setup_remote:
        if not remote_enabled(cfg):
            print(
                "[error] --setup-remote requires a remote section with "
                "server_host in the config file",
                file=sys.stderr,
            )
            return 1
        return setup_remote(cfg)

    project_root = find_project_root()

    # Create a fresh work directory for this run on a real (persistent)
    # filesystem. Do not fall back to tempfile's default location: /tmp is
    # commonly tmpfs, which would benchmark an in-memory filesystem instead
    # of durable storage.
    if cfg.work_dir is None:
        work_parent = project_root / "bench_cross_db_work"
        work_parent.mkdir(parents=True, exist_ok=True)
        cfg.work_dir = Path(tempfile.mkdtemp(prefix="run_", dir=work_parent))
    cfg.work_dir = cfg.work_dir.resolve()
    cfg.work_dir.mkdir(parents=True, exist_ok=True)
    print(f"[info] work directory: {cfg.work_dir}")

    # Resolve output_dir: absolute paths are used as-is; relative paths are
    # resolved against the work directory so that all artifacts live together.
    if not cfg.output_dir.is_absolute():
        cfg.output_dir = cfg.work_dir / cfg.output_dir

    cfg.output_dir.mkdir(parents=True, exist_ok=True)
    write_documentation(cfg.output_dir)

    print(f"[info] project root: {project_root}")

    # Remote (two-machine) mode: probe the server host's physical core count
    # once for the per-tier skip check (the client keeps using the local count
    # for its own taskset mask). The effective per-test config is serialized
    # for the server-side agent inside the test loop below.
    remote_mode = remote_enabled(cfg)
    server_physical_cores = physical_cores
    if remote_mode:
        remote = RemoteConfig(cfg.remote)
        probe = ParamikoSsh(remote)
        try:
            probe.connect()
            server_physical_cores = probe.remote_physical_cores()
            if any(
                key.startswith("mududb")
                for _, _, enabled in test_plans
                for key in enabled
            ):
                # Detect a missing server-side binary early with a hint,
                # instead of failing every mududb run in --server-run.
                mudud_path = f"{remote.server_project_root}/target/release/mudud"
                try:
                    probe.run_remote(f"test -x {shlex.quote(mudud_path)}")
                except RuntimeError:
                    print(
                        f"[warn] remote mudud binary {remote.server_host}:"
                        f"{mudud_path} is missing or not executable; MuduDB "
                        "backends will fail to start. 请先运行 --setup-remote "
                        "同步源码并在远端编译 (run --setup-remote first)",
                        file=sys.stderr,
                    )
        except RuntimeError as e:
            print(f"[error] {e}", file=sys.stderr)
            if not cfg.keep_data and cfg.work_dir is not None and cfg.work_dir.exists():
                shutil.rmtree(cfg.work_dir, ignore_errors=True)
            return 1
        finally:
            probe.close()
        print(
            f"[info] remote server {remote.server_host}: "
            f"{server_physical_cores} physical CPU core(s)"
        )

    # Determine client core mask if requested.
    client_cores: Optional[str] = None
    if cfg.client_cpu_cores is not None and cfg.client_cpu_cores:
        # Take the first entry as the fixed client core count.
        client_cores = build_core_mask(
            cfg.client_cpu_cores[0],
            offset=cfg.client_cpu_offset,
            available=physical_cores,
        )
        print(f"[info] client pinned to cores: {client_cores}")

    total_runs = 0
    failed_runs = 0

    try:
        for test_name, test_dict, enabled in test_plans:
            test_cfg = BenchConfig.from_dict(test_dict)
            test_cfg.work_dir = cfg.work_dir
            test_dir = cfg.output_dir / test_name
            test_cfg.output_dir = test_dir

            # Serialize the effective per-test config for the server-side
            # agent (a flat BenchConfig-compatible dict, as --server-run
            # expects).
            test_remote_config_text: Optional[str] = None
            if remote_mode:
                import yaml

                test_remote_config_text = yaml.safe_dump(
                    test_dict, default_flow_style=False
                )

            backends = build_backends(
                project_root,
                test_cfg,
                enabled,
                mudud_modes=mududb_modes,
                remote_config_text=test_remote_config_text,
            )

            print(f"\n{'='*60}")
            print(f"Test: {test_name}")
            print(f"{'='*60}")
            print(f"[info] backends: {', '.join(enabled)}")

            test_results: List[TpccResult] = []
            test_aggregates: List[Dict[str, Any]] = []

            for backend in backends:
                print(f"\n{'='*60}")
                print(f"Backend: {backend.name} / {backend.mode}")
                print(f"{'='*60}")

                for cores in test_cfg.cpu_cores:
                    if cores > server_physical_cores:
                        print(
                            f"[skip] requested {cores} cores but only {server_physical_cores} available"
                        )
                        continue

                    for connections in test_cfg.connections:
                        for hotspot in test_cfg.seckill_hotspot_percents:
                            for think_time in test_cfg.think_times_ms:
                                print(
                                    f"\n[config] cores={cores}, connections={connections}, "
                                    f"seckill_hotspot_percent={hotspot}, think_time_ms={think_time}"
                                )
                                run_results: List[TpccResult] = []

                                for run in range(1, test_cfg.repeats + 1):
                                    print(f"[run {run}/{test_cfg.repeats}] ...")
                                    try:
                                        res = run_single_benchmark(
                                            project_root,
                                            backend,
                                            test_cfg,
                                            cores,
                                            connections,
                                            run,
                                            client_cores,
                                            seckill_hotspot_percent=hotspot,
                                            think_time_ms=think_time,
                                        )
                                        run_results.append(res)
                                        print(
                                            f"[result] TPS={res.tps:.2f} CommTPS={res.committed_tps:.2f} P99={res.p99_latency_ms:.3f}ms"
                                        )
                                    except Exception as e:
                                        print(f"[error] run {run} failed: {e}", file=sys.stderr)
                                        test_results.append(
                                            TpccResult(
                                                backend=backend.name,
                                                mode=backend.mode,
                                                cores=cores,
                                                connections=connections,
                                                run=run,
                                                warehouses=test_cfg.warehouses,
                                                districts=test_cfg.districts,
                                                customers=test_cfg.customers,
                                                items=test_cfg.items,
                                                operations=test_cfg.operations,
                                                load_elapsed_sec=0.0,
                                                txn_elapsed_sec=0.0,
                                                total_elapsed_sec=0.0,
                                                throughput=0.0,
                                                tps=0.0,
                                                committed_tps=0.0,
                                                new_order_tps=0.0,
                                                total_throughput=0.0,
                                                op_count=0,
                                                abort_count=0,
                                                abort_rate_pct=0.0,
                                                avg_latency_ms=0.0,
                                                min_latency_ms=0.0,
                                                max_latency_ms=0.0,
                                                p50_latency_ms=0.0,
                                                p90_latency_ms=0.0,
                                                p99_latency_ms=0.0,
                                                p999_latency_ms=0.0,
                                                error=str(e),
                                                server_binary=getattr(
                                                    backend, "server_binary", ""
                                                ),
                                                seckill_hotspot_percent=hotspot,
                                                think_time_ms=think_time,
                                            )
                                        )
                                    finally:
                                        backend.stop()

                                test_results.extend(run_results)
                                if run_results:
                                    agg = aggregate(run_results)
                                    test_aggregates.append(agg)
                                    print(
                                        f"[aggregate] TPS={agg['tps_mean']:.2f}±{agg['tps_std']:.2f} "
                                        f"CommTPS={agg['committed_tps_mean']:.2f}±{agg['committed_tps_std']:.2f} "
                                        f"P99={agg['p99_mean']:.3f}±{agg['p99_std']:.3f}ms"
                                    )

            save_results(test_results, test_aggregates, test_dir)
            if test_aggregates:
                print(f"\n== test: {test_name} ==")
                print_summary_table(test_aggregates)
                plot_results(test_aggregates, test_dir)

            total_runs += len(test_results)
            failed_runs += sum(1 for r in test_results if r.error)

        timestamp = time.strftime("%Y%m%d_%H%M%S")
        persistent_dir = project_root / "bench_cross_db_results" / timestamp
        shutil.copytree(cfg.output_dir, persistent_dir)
        print(f"[output] persistent results: {persistent_dir}")

        if failed_runs:
            print(f"\n[warn] {failed_runs}/{total_runs} runs reported errors")
        return 0
    finally:
        if not cfg.keep_data and cfg.work_dir is not None and cfg.work_dir.exists():
            shutil.rmtree(cfg.work_dir, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
