#!/usr/bin/env python3
"""Merge TPC-C cross-database benchmark results from multiple runs.

Use case: a full run produced valid results for some backends (e.g. MuduDB,
SpacetimeDB) but failed for others; after fixing the issue you re-run only
the failed backends with `--backends postgres,mysql` and merge the new
results into the old ones, instead of re-running everything. Re-running a
single (cores, connections) combination to fill a gap works too: only that
combination's rows are overridden.

Each positional argument is a result directory (containing results.json, or
the newer per-test layout with one results.json per test subdirectory) or a
results.json file. Files loaded later override earlier ones per
(backend, mode, cores, connections, seckill_hotspot_percent, think_time_ms):
rows for a combination present in a later file replace the rows from earlier
files. Rows with a non-empty `error` field (failed-run placeholders) are
always dropped.

Aggregation, summary table, CSV, and plots are produced by reusing the
functions from bench_cross_db.py, so the merged output is identical in
format and aggregation semantics to a single full run.

Usage:
    cd example/tpcc
    python3 bench_merge_results.py \\
        ../bench_cross_db_results/20260802_192325 \\
        ../bench_cross_db_results/20260802_230000 \\
        --output-dir ../bench_cross_db_results/merged_scalability
"""

import argparse
import json
import sys
from pathlib import Path
from typing import Dict, List, Tuple

# Allow running from any working directory: import the sibling module.
sys.path.insert(0, str(Path(__file__).resolve().parent))

from bench_cross_db import (  # noqa: E402
    TpccResult,
    aggregate,
    plot_results,
    print_summary_table,
    save_results,
)


def load_results_file(path: Path) -> List[TpccResult]:
    """Load one results.json into TpccResult records.

    A directory argument uses its results.json; if it has none, the per-test
    layout is assumed and all `<dir>/*/results.json` files are loaded.
    """
    if path.is_dir():
        direct = path / "results.json"
        if direct.exists():
            results_paths = [direct]
        else:
            results_paths = sorted(path.glob("*/results.json"))
            if not results_paths:
                raise RuntimeError(f"results file not found: {direct}")
    else:
        results_paths = [path]
    results: List[TpccResult] = []
    for results_path in results_paths:
        raw = json.loads(results_path.read_text(encoding="utf-8"))
        if not isinstance(raw, list):
            raise RuntimeError(f"unexpected results format in {results_path}")
        results.extend(TpccResult(**row) for row in raw)
    return results


def merge_results(file_rows: List[List[TpccResult]]) -> List[TpccResult]:
    """Merge per-file rows; later files override per combination.

    The override key is (backend, mode, cores, connections,
    seckill_hotspot_percent, think_time_ms), so re-running a single sweep
    combination only replaces that combination's rows. Failed-run
    placeholder rows (non-empty error) are dropped. Row order follows first
    appearance so grouping keeps a stable, readable order.
    """
    merged: List[TpccResult] = []
    for rows in file_rows:
        rows = [r for r in rows if not r.error]
        if not rows:
            continue
        overridden = {
            (
                r.backend,
                r.mode,
                r.cores,
                r.connections,
                r.seckill_hotspot_percent,
                r.think_time_ms,
            )
            for r in rows
        }
        merged = [
            r
            for r in merged
            if (
                r.backend,
                r.mode,
                r.cores,
                r.connections,
                r.seckill_hotspot_percent,
                r.think_time_ms,
            )
            not in overridden
        ]
        merged.extend(rows)
    return merged


def aggregate_all(results: List[TpccResult]) -> List[Dict]:
    """Aggregate per (backend, mode, cores, connections,
    seckill_hotspot_percent, think_time_ms), preserving order."""
    groups: Dict[Tuple[str, str, int, int, int, int], List[TpccResult]] = {}
    for r in results:
        key = (
            r.backend,
            r.mode,
            r.cores,
            r.connections,
            r.seckill_hotspot_percent,
            r.think_time_ms,
        )
        groups.setdefault(key, []).append(r)
    return [aggregate(rows) for rows in groups.values()]


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Merge TPC-C benchmark results from multiple runs",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "inputs",
        type=Path,
        nargs="+",
        help="Result directories (containing results.json) or results.json "
        "files, in override order: later inputs win per backend/mode",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        required=True,
        help="Directory for the merged results.json / summary.csv / plots",
    )
    args = parser.parse_args()

    try:
        file_rows = [load_results_file(p) for p in args.inputs]
    except RuntimeError as e:
        print(f"[error] {e}", file=sys.stderr)
        return 1

    for path, rows in zip(args.inputs, file_rows):
        valid = sum(1 for r in rows if not r.error)
        print(f"[info] {path}: {valid}/{len(rows)} valid rows")

    merged = merge_results(file_rows)
    if not merged:
        print("[error] no valid rows to merge", file=sys.stderr)
        return 1

    backends = sorted({(r.backend, r.mode) for r in merged})
    print(f"[info] merged {len(merged)} rows, backends: "
          + ", ".join(f"{b}/{m}" for b, m in backends))

    aggregates = aggregate_all(merged)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    save_results(merged, aggregates, args.output_dir)
    print_summary_table(aggregates)
    plot_results(aggregates, args.output_dir)
    print(f"[output] merged results: {args.output_dir}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
