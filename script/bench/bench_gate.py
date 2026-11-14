#!/usr/bin/env python3
"""Run one benchmark at smoke scale and check it against its committed baseline.

This is the single entry point wired into `cargo make bench-smoke*`; the
smoke parameters live here exactly once so the check and --update flows can
never drift apart.

Usage:
  bench_gate.py paged-index [--update]
  bench_gate.py tpcc [--update]

Without --update the run is compared against
script/bench/baselines/<benchmark>.json and exits nonzero on any gate
violation; with --update the run re-records that baseline (review the diff
in the same commit, same convention as binding regen).
"""
import argparse
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "script" / "lib"))
from output import cmd, error, highlight  # noqa: E402

PAGED_OUTPUT = "target/bench/gate-paged"
TPCC_OUTPUT = "target/bench/gate-tpcc"
TPCC_MPK = "target/wasm32-wasip2/release/tpcc.mpk"


def run(argv) -> None:
    cmd(argv)
    subprocess.check_call([str(a) for a in argv], cwd=ROOT)


def compare(benchmark: str, results: str, update: bool) -> None:
    argv = [sys.executable, "script/bench/compare_baseline.py",
            "--benchmark", benchmark, "--results", results]
    if update:
        argv.append("--update")
    run(argv)


def paged_index(update: bool) -> None:
    run([sys.executable, "script/bench/paged_index.py", "--suite", "tasks",
         "--no-plot", "--ops", "1000", "--repeats", "1", "--output", PAGED_OUTPUT])
    compare("paged-index", f"{PAGED_OUTPUT}/summary.json", update)


def tpcc(update: bool) -> None:
    if not (ROOT / TPCC_MPK).is_file():
        error(f"missing {highlight(TPCC_MPK)}; build it first with `cargo make package-tpcc`")
        raise SystemExit(2)
    run([sys.executable, "script/bench/tpcc_benchmark.py",
         "--mode", "stored-procedure", "--server-mode", "tokio",
         "--warehouses", "1", "--operations", "300", "--connections", "2",
         "--runs", "3", "--mpk-path", TPCC_MPK, "--output-dir", TPCC_OUTPUT])
    compare("tpcc", f"{TPCC_OUTPUT}/benchmark_summary.json", update)


GATES = {"paged-index": paged_index, "tpcc": tpcc}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("benchmark", choices=sorted(GATES))
    parser.add_argument("--update", action="store_true",
                        help="re-record the baseline instead of checking against it")
    args = parser.parse_args(argv)
    GATES[args.benchmark](args.update)
    return 0


if __name__ == "__main__":
    sys.exit(main())
