#!/usr/bin/env python3
"""Compare a benchmark result file against its committed baseline.

Baselines live in `script/bench/baselines/<benchmark>.json` and are
self-describing: every metric carries a gate that decides how the actual
value is judged:

  exact     - deterministic counter; must equal the baseline value
  band      - actual must stay within [value*min_ratio, value*max_ratio]
  delta_max - actual must stay at or below value + delta
  min       - actual must be at or above value (a sanity floor)
  info      - recorded for trend inspection only; never fails

Deterministic counters (page reads/writes/evictions, row counts) use
"exact". Wall-clock metrics (ops/s, TPS, latencies) are machine-dependent:
keep them "info" or a wide "band"/"min" so the gate survives machine
differences between developer boxes and CI runners.

Usage:
  compare_baseline.py --benchmark paged-index --results <dir>/summary.json
  compare_baseline.py --benchmark tpcc --results <dir>/benchmark_summary.json
  compare_baseline.py --benchmark paged-index --results ... --update

Exit status: 0 when every hard gate passes (or after --update), 1 on gate
failure, 2 on usage/IO errors.
"""
import argparse
import copy
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "script" / "lib"))
from output import bad, error, good, highlight, status, warn  # noqa: E402

BASELINE_DIR = ROOT / "script" / "bench" / "baselines"

# Default gates assigned when --update meets a metric the old baseline did
# not carry. Anything not listed defaults to "info".
DEFAULT_GATES = {
    "paged-index": {
        "reads": {"gate": "exact"},
        "writes": {"gate": "exact"},
        "evictions": {"gate": "exact"},
        "rows": {"gate": "exact"},
        # Hint hits depend on cooperative task interleaving; observed
        # cross-run spread is ~0.14%, so a narrow band stays flake-free.
        "hint_hits": {"gate": "band", "min_ratio": 0.98, "max_ratio": 1.02},
        "paged_over_baseline_ops": {"gate": "band", "min_ratio": 0.3, "max_ratio": 3.0},
    },
    "tpcc": {
        "abort_rate_pct": {"gate": "delta_max", "delta": 5.0},
        # Catastrophic-collapse floor only: a weaker CI runner must never
        # flake, so the floor sits far below any healthy machine's TPS.
        "tps": {"gate": "min", "shrink": 0.1},
        "op_count": {"gate": "min", "shrink": 0.9},
    },
}

PAGED_SCALE_KEYS = [
    "suite", "ops", "repeats", "tasks", "keys", "key_bytes",
    "page_size", "frames", "task_frames",
]
TPCC_SCALE_KEYS = [
    "mode", "warehouses", "districts", "customers", "items",
    "operations", "connections",
]

PAGED_ENTRY_METRICS = [
    "seconds", "ops_s", "p50_us", "p95_us", "p99_us", "load_ms",
    "preload_flush_ms", "reads", "writes", "evictions", "hint_hits", "rows",
]
TPCC_ENTRY_METRICS = [
    "load_elapsed_sec", "txn_elapsed_sec", "throughput", "tps",
    "committed_tps", "new_order_tps", "op_count", "abort_count",
    "abort_rate_pct", "avg_latency_ms", "p50_latency_ms", "p90_latency_ms",
    "p99_latency_ms", "p999_latency_ms",
]


def load_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        error(f"file not found: {highlight(path)}")
        raise SystemExit(2)
    except json.JSONDecodeError as exc:
        error(f"invalid JSON in {highlight(path)}: {exc}")
        raise SystemExit(2)


def extract_paged_index(results_path: Path):
    """Normalize paged-index summary.json into scale + entries.

    Entries are keyed `<case>/<backend>`; a derived `ratio/<case>` entry
    carries the same-run paged/baseline throughput ratio, which cancels
    most machine noise.
    """
    rows = load_json(results_path)
    if not isinstance(rows, list) or not rows:
        error(f"{highlight(results_path)} holds no measurements")
        raise SystemExit(2)
    metadata_path = results_path.parent / "metadata.json"
    metadata = load_json(metadata_path) if metadata_path.exists() else {}
    arguments = metadata.get("arguments", {})
    scale = {key: arguments[key] for key in PAGED_SCALE_KEYS if key in arguments}
    entries = {}
    for row in rows:
        key = f"{row['case']}/{row['backend']}"
        entries[key] = {m: float(row[m]) for m in PAGED_ENTRY_METRICS if m in row}
    by_case = {}
    for row in rows:
        by_case.setdefault(row["case"], {})[row["backend"]] = row
    for case, pair in sorted(by_case.items()):
        if set(pair) == {"baseline", "paged"} and float(pair["baseline"]["ops_s"]) > 0:
            ratio = float(pair["paged"]["ops_s"]) / float(pair["baseline"]["ops_s"])
            entries[f"ratio/{case}"] = {"paged_over_baseline_ops": ratio}
    extras = {"source_sha256": metadata.get("source_sha256")}
    return scale, entries, extras


def extract_tpcc(results_path: Path):
    """Normalize tpcc benchmark_summary.json into scale + per-backend entries."""
    summary = load_json(results_path)
    config = summary.get("config", {})
    results = summary.get("results", {})
    if not results or not any(results.values()):
        error(f"{highlight(results_path)} holds no benchmark runs")
        raise SystemExit(2)
    scale = {key: config[key] for key in ("mode", "warehouses", "operations", "connections") if key in config}
    first_run = next(r for runs in results.values() for r in runs)
    for key in ("districts", "customers", "items"):
        if key in first_run:
            scale[key] = first_run[key]
    entries = {}
    for backend, runs in sorted(results.items()):
        if not runs:
            continue
        entry = {}
        for metric in TPCC_ENTRY_METRICS:
            values = [float(r[metric]) for r in runs if metric in r]
            if values:
                entry[metric] = sum(values) / len(values)
        entries[backend] = entry
    return scale, entries, {}


EXTRACTORS = {"paged-index": extract_paged_index, "tpcc": extract_tpcc}


def evaluate(gate: dict, actual: float):
    """Return (verdict, expectation): verdict is True/False or None for info."""
    kind = gate.get("gate", "info")
    value = float(gate.get("value", 0.0))
    if kind == "info":
        return None, "recorded only"
    if kind == "exact":
        return actual == value, f"== {value:g}"
    if kind == "band":
        lo = value * float(gate.get("min_ratio", 0.5))
        hi = value * float(gate.get("max_ratio", 2.0))
        return lo <= actual <= hi, f"within [{lo:g}, {hi:g}]"
    if kind == "delta_max":
        limit = value + float(gate.get("delta", 0.0))
        return actual <= limit, f"<= {limit:g} (baseline {value:g} + {gate.get('delta', 0.0):g})"
    if kind == "min":
        return actual >= value, f">= {value:g}"
    error(f"unknown gate kind {kind!r}")
    raise SystemExit(2)


def compare(baseline: dict, scale: dict, entries: dict) -> bool:
    """Diff actual entries against the baseline; return True when all pass."""
    failures, rows = [], []
    base_entries = baseline.get("entries", {})
    base_scale = baseline.get("scale", {})
    for key in sorted(set(base_scale) | set(scale)):
        if base_scale.get(key) != scale.get(key):
            failures.append(
                f"scale {key!r}: baseline={base_scale.get(key)!r} actual={scale.get(key)!r} "
                f"(regenerate with --update when the change is intended)"
            )
    for name in sorted(set(base_entries) | set(entries)):
        if name not in base_entries:
            warn(f"new entry {name!r} has no baseline; record it with --update")
            continue
        if name not in entries:
            failures.append(f"entry {name!r} is in the baseline but missing from the results")
            continue
        base_metrics, actual_metrics = base_entries[name], entries[name]
        for metric in sorted(set(base_metrics) | set(actual_metrics)):
            if metric not in base_metrics:
                warn(f"new metric {name}/{metric} has no baseline; record it with --update")
                continue
            if metric not in actual_metrics:
                failures.append(f"metric {name}/{metric} is in the baseline but missing from the results")
                continue
            gate = base_metrics[metric]
            actual = float(actual_metrics[metric])
            verdict, expectation = evaluate(gate, actual)
            rows.append((name, metric, gate.get("gate", "info"), expectation, actual, verdict))
            if verdict is False:
                failures.append(
                    f"{name}/{metric}: expected {expectation}, got {actual:g}"
                )
    width = max((len(f"{n}/{m}") for n, m, *_ in rows), default=0)
    for name, metric, kind, expectation, actual, verdict in rows:
        if verdict is None:
            mark, render = "info", lambda t: t
        elif verdict:
            mark, render = "pass", good
        else:
            mark, render = "FAIL", bad
        print(f"  {render(f'{mark:>4}')}  {f'{name}/{metric}':<{width}}  "
              f"actual={actual:<14g} gate={kind} ({expectation})")
    for failure in failures:
        error(failure)
    return not failures


def updated_baseline(benchmark: str, old: dict, scale: dict, entries: dict, extras: dict) -> dict:
    """Record actual values as the new baseline, preserving gate definitions."""
    defaults = DEFAULT_GATES.get(benchmark, {})
    old_entries = old.get("entries", {})
    new_entries = {}
    for name in sorted(entries):
        metrics = {}
        for metric in sorted(entries[name]):
            observed = float(entries[name][metric])
            prior = old_entries.get(name, {}).get(metric)
            gate = copy.deepcopy(prior) if prior else copy.deepcopy(defaults.get(metric, {"gate": "info"}))
            if gate.get("gate") == "min":
                gate["value"] = observed * float(gate.get("shrink", 0.3))
                gate.pop("shrink", None)
            else:
                gate["value"] = observed
            metrics[metric] = gate
        new_entries[name] = metrics
    return {
        "benchmark": benchmark,
        "scale": scale,
        "source_sha256": extras.get("source_sha256") or old.get("source_sha256"),
        "entries": new_entries,
    }


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--benchmark", required=True, choices=sorted(EXTRACTORS),
                        help="which benchmark produced --results")
    parser.add_argument("--results", type=Path, required=True,
                        help="result file written by the benchmark runner")
    parser.add_argument("--baseline", type=Path, default=None,
                        help=f"baseline file (default: {BASELINE_DIR}/<benchmark>.json)")
    parser.add_argument("--update", action="store_true",
                        help="record the actual results as the new baseline instead of comparing")
    args = parser.parse_args(argv)

    baseline_path = args.baseline or (BASELINE_DIR / f"{args.benchmark}.json")
    scale, entries, extras = EXTRACTORS[args.benchmark](args.results.resolve())

    if args.update:
        old = load_json(baseline_path) if baseline_path.exists() else {}
        baseline = updated_baseline(args.benchmark, old, scale, entries, extras)
        baseline_path.parent.mkdir(parents=True, exist_ok=True)
        baseline_path.write_text(json.dumps(baseline, indent=2) + "\n", encoding="utf-8")
        status("Updated", f"{highlight(baseline_path)}: "
               f"{len(entries)} entries, {sum(len(m) for m in entries.values())} metrics")
        return 0

    baseline = load_json(baseline_path)
    if baseline.get("benchmark") != args.benchmark:
        error(f"{highlight(baseline_path)} is a {baseline.get('benchmark')!r} baseline, "
              f"not {args.benchmark!r}")
        return 2
    stale = extras.get("source_sha256") and baseline.get("source_sha256") and \
        extras["source_sha256"] != baseline["source_sha256"]
    if stale:
        changed = sorted(k for k in set(extras["source_sha256"]) | set(baseline["source_sha256"])
                         if extras["source_sha256"].get(k) != baseline["source_sha256"].get(k))
        warn(f"benchmark sources changed since the baseline was recorded "
             f"({len(changed)} files, e.g. {changed[0]}); "
             f"if the change is intended, regenerate with --update")
    status("Comparing", f"{args.results.resolve()} against {highlight(baseline_path)}")
    if compare(baseline, scale, entries):
        status("Passed", "all benchmark gates hold")
        return 0
    error("benchmark regression gate failed; inspect the table above, or record an "
          "intended change with --update")
    return 1


if __name__ == "__main__":
    sys.exit(main())
