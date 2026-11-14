#!/usr/bin/env python3
"""Benchmark paged B+ tree versus original BTreeIndex and generate reports/plots."""
import argparse
import csv
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import time
import tempfile
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "script" / "lib"))
from output import error, format_duration, highlight, status  # noqa: E402
import paged_index_analysis as analysis
from paged_index_output import print_summary, print_trial  # noqa: E402

FIELDS = "case backend trial workload keys tasks key_bytes prefix hot page_size frames hints prefetch file cold generic lexical seconds ops_s p50_us p95_us p99_us load_ms preload_flush_ms reads writes evictions hint_hits rows".split()


def command_output(*args):
    result = subprocess.run(args, cwd=ROOT, text=True, capture_output=True, check=False)
    return result.stdout.strip()


def metadata(args, command):
    sources = {}
    for directory in ("index", "page_buffer"):
        for path in sorted((ROOT / "crates/db-kernel" / directory).rglob("*.rs")):
            sources[str(path.relative_to(ROOT))] = hashlib.sha256(path.read_bytes()).hexdigest()
    return {
        "utc": datetime.now(timezone.utc).isoformat(), "command": command,
        "execution_model": "partition-local-v1", "runtime_threads": 1,
        "arguments": vars(args), "platform": platform.platform(),
        "cpu_count": os.cpu_count(), "cpu": command_output("lscpu"),
        "rustc": command_output("rustc", "-Vv"),
        "git_head": command_output("git", "rev-parse", "HEAD"),
        "git_status": command_output("git", "status", "--short"),
        "source_sha256": sources if not args.executable else None,
        "executable_sha256": hashlib.sha256(Path(args.executable).read_bytes()).hexdigest() if args.executable else None,
        "notes": "Closed-loop, one Tokio current-thread runtime with local tasks; no CPU isolation or OS cache eviction. Frame budget is not RSS.",
    }


def summarize(rows, output):
    groups = {}
    for row in rows:
        groups.setdefault((row["case"], row["backend"]), []).append(row)
    summary = []
    for (case, backend), samples in groups.items():
        item = {"case": case, "backend": backend, "trials": len(samples)}
        for field in FIELDS[FIELDS.index("seconds"): ]:
            item[field] = statistics.median(float(s[field]) for s in samples)
        rates = [float(s["ops_s"]) for s in samples]
        item.update(min_ops_s=min(rates), max_ops_s=max(rates))
        summary.append(item)
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    by_case = {}
    for row in summary:
        by_case.setdefault(row["case"], {})[row["backend"]] = row
    lines = ["# Paged index benchmark", "", "Medians across trials; throughput range is min–max. Latency columns are medians of per-trial percentiles, not pooled percentiles.", "",
             "Baseline is the original BTreeMap-backed BTreeIndex accessed directly on its owning thread (no outer async mutex). Paged uses inline u128 values except in generic-values. Both execute identical deterministic traces in one current-thread runtime. Task count measures cooperative concurrency, not CPU parallelism; each task yields every 64 operations.", "",
             "| Case | Baseline ops/s | Paged ops/s (min–max) | Paged / baseline | Baseline p99 µs | Paged p99 µs |", "| --- | ---: | ---: | ---: | ---: | ---: |"]
    for case, pair in by_case.items():
        if set(pair) != {"baseline", "paged"}:
            # A profiling run may intentionally evaluate only one backend.
            for backend, row in pair.items():
                rate = f'{row["ops_s"]:,.0f}'
                p99 = f'{row["p99_us"]:.2f}'
                if backend == "baseline":
                    lines.append(f'| {case} | {rate} | — | — | {p99} | — |')
                else:
                    span = f'{row["min_ops_s"]:,.0f}–{row["max_ops_s"]:,.0f}'
                    lines.append(f'| {case} | — | {rate} ({span}) | — | — | {p99} |')
            continue
        old, new = pair["baseline"], pair["paged"]
        lines.append(f'| {case} | {old["ops_s"]:,.0f} | {new["ops_s"]:,.0f} ({new["min_ops_s"]:,.0f}–{new["max_ops_s"]:,.0f}) | {new["ops_s"] / old["ops_s"]:.3f} | {old["p99_us"]:.2f} | {new["p99_us"]:.2f} |')
    lines += ["", "## Paged setup and measured-region cache traffic", "",
              "Load is sequential insertion, excluding key/trace construction. Preload flush is outside operation timing; it includes buffer flush, and fsync for file backing. It is not per-operation durability.", "",
              "| Case | Load ms | Preload flush ms | Reads | Writes | Evictions | Hint hits |",
              "| --- | ---: | ---: | ---: | ---: | ---: | ---: |"]
    for case, pair in by_case.items():
        if "paged" in pair:
            r = pair["paged"]
            lines.append(f'| {case} | {r["load_ms"]:.2f} | {r["preload_flush_ms"]:.2f} | {r["reads"]:.0f} | {r["writes"]:.0f} | {r["evictions"]:.0f} | {r["hint_hits"]:.0f} |')
    lines += ["", "File cold-start resets only the application cache. MemoryPageStore retains backing bytes in RAM. Neither cache capacity nor these counters measures process RSS. Scans report operations/s; multiply by scan width for rows/s. Read/write mixes use replacements, not transactions. Timing includes latency instrumentation, assertions and scheduling; this is a closed-loop microbenchmark, not a WAL/SQL/MVCC or storage-device benchmark.", ""]
    (output / "summary.md").write_text("\n".join(lines))


class HelpFormatter(argparse.ArgumentDefaultsHelpFormatter, argparse.RawDescriptionHelpFormatter):
    """Show defaults while preserving the runnable examples in the epilog."""


def parse_args(argv=None):
    """Parse benchmark options; expensive work starts only after validation."""
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=HelpFormatter,
        epilog="""Examples (run from the repository root):
  python3 script/bench/paged_index.py
  python3 script/bench/paged_index.py --suite tasks --case read-t4 --ops 100000 --repeats 5
  python3 script/bench/paged_index.py --suite tasks --backend paged --diagnostics --case changed-update-t4
  python3 script/bench/paged_index.py --no-plot --output target/bench/custom
  python3 script/bench/paged_index.py --suite cache --ops 20000
  python3 script/bench/paged_index.py --suite cache --diagnostics --allocations
  python3 script/bench/paged_index.py --suite cache --case cache-scan256-f4096 --backend paged --repeats 1 --ops 100000 --profile

Task-suite cases (use a count listed in --tasks):
  read-t1, read-t4, mixed90-t4, mixed50-t4, changed-update-t4,
  insert-t16, delete-t16, scan32-t4, scan256-t4
Matrix cases (--suite matrix):
  read, miss, update, insert, delete, scan32, scan256,
  key-bytes-128, lexical-2048, generic-values, file-pressure

Outputs: summary.md, summary.json, results.csv, metadata.json, raw.log,
         comparison.png and comparison.svg; diagnostics.csv when enabled.
Plotting requires matplotlib: python3 -m pip install matplotlib
If a WASI clang precedes native clang, prefix the command with:
  PATH=/usr/bin:/bin:$PATH CLANG_PATH=/usr/bin/clang
""")
    parser.add_argument("--ops", type=int, default=5000,
                        help="operations per task; delete is capped at keys/tasks")
    parser.add_argument("--repeats", type=int, default=3,
                        help="trials per case/backend; reports use medians")
    parser.add_argument("--executable", metavar="PATH",
                        help="retained benchmark executable; omitted means build with cargo --release")
    parser.add_argument("--diagnostics", action="store_true",
                        help="enable work counters; use separate runs from throughput measurements")
    parser.add_argument("--backend", choices=["both", "paged", "baseline"], default="both",
                        help="backends to evaluate; baseline is the original implementation")
    parser.add_argument("--case", metavar="NAME",
                        help="exact case name; omitted runs all cases in the selected suite")
    parser.add_argument("--output", metavar="DIR",
                        default="target/bench/paged-index-" + datetime.now().strftime("%Y%m%d-%H%M%S-%f"),
                        help="output directory; timestamped by default to preserve earlier runs")
    parser.add_argument("--plot", action=argparse.BooleanOptionalAction, default=True,
                        help="generate evaluation plots in PNG and SVG formats")
    parser.add_argument("-v", "--verbose", action="store_true",
                        help="stream raw benchmark records and test harness output as well")
    analysis.add_options(parser)
    args = parser.parse_args(argv)
    analysis.validate_options(parser, args)
    if args.ops < 1 or args.repeats < 1:
        parser.error("ops and repeats must be positive")
    if args.executable and not Path(args.executable).is_file():
        parser.error(f"benchmark executable does not exist: {args.executable}")
    return args


def main(argv=None):
    args = parse_args(argv)
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    status("Benchmark", f"{args.case or (args.suite + ' suite')}; backend={args.backend}; "
           f"{args.ops:,} ops/task; {args.repeats} trials/backend; diagnostics={args.diagnostics}")
    status("Output", highlight(output))
    sys.stdout.flush()
    if args.plot:
        # Fail before building/running the matrix if plotting dependencies are absent.
        from paged_index_plot import load_pyplot, plot_report
        try:
            pyplot = load_pyplot(output)
        except ImportError as exc:
            raise SystemExit("Plotting requires matplotlib: python3 -m pip install matplotlib; "
                             "or pass --no-plot.") from exc
    if args.profile and not args.executable:
        status("Building", "release executable with measured-window CPU profiling")
        args.executable = analysis.profile_executable(ROOT)
    command = ["cargo", "test", "--release", "--manifest-path", "crates/db-kernel/Cargo.toml", "-p", "index",
               "paged::bench_matrix::performance_matrix", "--", "--exact", "--ignored", "--nocapture", "--test-threads=1"]
    if args.allocations:
        command[command.index("--"):command.index("--")] = ["--features", "bench-alloc"]
    if args.executable:
        command = [str(Path(args.executable).resolve()), "paged::bench_matrix::performance_matrix", "--exact", "--ignored", "--nocapture", "--test-threads=1"]
    status("Preparing", "recording environment and source metadata")
    (output / "metadata.json").write_text(json.dumps(metadata(args, command), indent=2) + "\n")
    env = os.environ.copy()
    analysis.configure_environment(env, args, output)
    env.update(MUDU_BENCH_OPS=str(args.ops), MUDU_BENCH_REPEATS=str(args.repeats))
    env.pop("MUDU_BENCH_CASE", None)
    if args.case:
        env["MUDU_BENCH_CASE"] = args.case
    for name, value in [("MUDU_BENCH_DIAGNOSTICS", "1" if args.diagnostics else None), ("MUDU_BENCH_BACKEND", None if args.backend == "both" else args.backend)]:
        if value is None:
            env.pop(name, None)
        else:
            env[name] = value
    status("Running" if args.executable else "Building",
           "retained benchmark executable" if args.executable else "release benchmark via cargo test")
    sys.stdout.flush()
    rows, diagnostic_rows, cache_rows, allocation_rows = [], [], [], []
    local_model = False
    diagnostic_fields = "case backend trial retries cow_bytes moved_bytes overflow_reads latch_waits latch_wait_ns latch_probe_hits checksum_calls checksum_bytes checksum_ns writeback_copy_bytes".split()
    # Dedicated temporary backing files; never truncate an existing user file.
    with tempfile.TemporaryDirectory(prefix="pages-", dir=output) as directory:
        env["MUDU_BENCH_DIR"] = directory
        with (output / "raw.log").open("w") as log:
            process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
            for line in process.stdout:
                log.write(line)
                log.flush()
                # Keep compiler diagnostics and failures visible; hide only known
                # machine records and routine libtest boilerplate by default.
                text = line.strip()
                if "MODEL,partition-local-v1" in line:
                    local_model = True
                record = any(prefix in line for prefix in ("DIAG,", "BENCH,", "CACHE,", "ALLOC,", "MODEL,"))
                harness = (not text or text == "ok" or text == "running 1 test"
                           or text.startswith("test result: ok.")
                           or text == "test paged::bench_matrix::performance_matrix ... ok")
                if args.verbose or not (record or harness):
                    print(line, end="", flush=True)
                for prefix, fields, records in [("CACHE,", analysis.CACHE_FIELDS, cache_rows), ("ALLOC,", analysis.ALLOC_FIELDS, allocation_rows)]:
                    if prefix in line:
                        values = next(csv.reader(io.StringIO(line.split(prefix, 1)[1])))
                        if len(values) != len(fields):
                            raise ValueError(f"{prefix} column mismatch")
                        records.append(dict(zip(fields, values)))
                if "DIAG," in line:
                    values = next(csv.reader(io.StringIO(line.split("DIAG,", 1)[1])))
                    # Retained executables may predate latch and checksum counters.
                    # Leave their new fields empty rather than inventing zeroes.
                    if len(values) not in (len(diagnostic_fields), 10, 9, 7):
                        raise ValueError("diagnostic column mismatch")
                    diagnostic_rows.append(dict(zip(diagnostic_fields, values)))
                if "BENCH," in line:
                    values = next(csv.reader(io.StringIO(line.split("BENCH,", 1)[1])))
                    if len(values) != len(FIELDS):
                        raise ValueError(f"benchmark column mismatch: {len(values)} != {len(FIELDS)}")
                    rows.append(dict(zip(FIELDS, values)))
                    print_trial(rows[-1], args.repeats, len(rows))
            returncode = process.wait()
    with (output / "results.csv").open("w", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=FIELDS)
        writer.writeheader()
        writer.writerows(rows)
    if diagnostic_rows:
        with (output / "diagnostics.csv").open("w", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=diagnostic_fields)
            writer.writeheader()
            writer.writerows(diagnostic_rows)
    analysis.write_records(output, 'cache_samples.csv', analysis.CACHE_FIELDS, cache_rows)
    analysis.write_records(output, 'allocations.csv', analysis.ALLOC_FIELDS, allocation_rows)
    if rows and not local_model:
        raise SystemExit("executable does not declare the partition-local-v1 model; rebuild it")
    if any(row["backend"] not in ("paged", "baseline") for row in rows):
        raise SystemExit("obsolete backend schema; rebuild the benchmark executable")
    if rows:
        summarize(rows, output)
    if returncode or not rows:
        error(f"benchmark failed (exit={returncode}, measurements={len(rows)}); "
              f"partial data: {output / 'results.csv'}; full log: {output / 'raw.log'}")
        raise SystemExit(returncode or 1)
    if args.suite == 'tasks':
        from paged_index_tasks import report_tasks
        report_tasks(rows, output)
    if args.suite == 'cache':
        if len(cache_rows) != len(rows):
            raise SystemExit('cache suite requires a current executable with CACHE records')
        analysis.analyze_cache(rows, cache_rows, output)
    if args.allocations and len(allocation_rows) != len(rows):
        raise SystemExit('allocation collection requires an executable built with bench-alloc')
    if diagnostic_rows or allocation_rows:
        analysis.diagnostic_report(rows, cache_rows, diagnostic_rows, allocation_rows, output)
    if args.profile:
        if not (output / 'cpu.prof').exists():
            raise SystemExit('profile missing; retained executable must enable bench-profile')
        result = subprocess.run([sys.executable, str(ROOT / 'script/bench/paged_index_profile.py'),
                                 str(output / 'cpu.prof'), args.executable, '--measured-window'],
                                capture_output=True, text=True)
        (output / 'cpu.txt').write_text(result.stdout + result.stderr)
        if result.returncode:
            raise SystemExit(f'CPU profile could not be summarized; see {output / "cpu.txt"}; increase --keys/--ops')
        with (output / 'summary.md').open('a') as report:
            report.write('\n## Measured-window CPU profile\n\nIncludes operation tasks and harness; excludes preload, warmup, final validation and teardown. Sample shares are not wall-clock fractions.\n\n```text\n' + result.stdout + '\n```\n')
    if args.allocations or args.diagnostics or args.profile:
        with (output / 'summary.md').open('a') as report:
            report.write('\nInstrumented run: use a separate uninstrumented run for throughput comparisons.\n')
    status("Evaluating", "median throughput and per-trial p99 latency")
    print_summary(output)
    if args.plot:
        status("Plotting", "comparison.png and comparison.svg")
        sys.stdout.flush()
        plot_report(output, pyplot=pyplot)
    status("Finished", f"{len(rows)} measurements across {len({row['case'] for row in rows})} cases "
           f"in {format_duration(time.monotonic() - started)}")
    artifacts = [("Report", "summary.md"), ("Data", "results.csv"), ("Log", "raw.log")]
    if args.plot:
        artifacts += [("Plot", "comparison.png"), ("Plot", "comparison.svg")]
    if diagnostic_rows:
        artifacts.append(("Diagnostics", "diagnostics.csv"))
    if args.suite == 'cache':
        artifacts += [('Cache analysis', 'cache_analysis.md'), ('Cache data', 'cache_analysis.csv')]
    if allocation_rows:
        artifacts += [('Allocations', 'allocations.csv'), ('Work stats', 'work_stats.json')]
    if args.suite == 'tasks':
        artifacts += [('Task analysis', 'task_analysis.csv')]
    if args.profile:
        artifacts += [('CPU profile', 'cpu.txt'), ('CPU samples', 'cpu.prof')]
    for label, name in artifacts:
        status(label, highlight(output / name))


if __name__ == "__main__":
    main()
