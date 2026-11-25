#!/usr/bin/env python3
"""Matched-capacity analysis and separate benchmark instrumentation controls."""
import csv
import json
from pathlib import Path
import statistics
import subprocess
import sys

CACHE_FIELDS = 'case backend trial pages operations'.split()
ALLOC_FIELDS = 'case backend trial alloc_calls alloc_bytes realloc_calls'.split()


def add_options(parser):
    parser.add_argument('--suite', choices=['tasks', 'matrix', 'cache'], default='tasks',
                        help='tasks compares local task counts; cache varies frame capacities')
    parser.add_argument('--tasks', default='1,4,16', help='task-suite concurrency levels on one executor thread')
    parser.add_argument('--task-frames', type=int, default=4096, help='fixed frame capacity for the task suite')
    parser.add_argument('--keys', type=int, default=4096, help='task/cache-suite dataset size (>256)')
    parser.add_argument('--key-bytes', type=int, default=8, help='task/cache-suite key width (>=8)')
    parser.add_argument('--page-size', type=int, default=4096, help='task/cache-suite page size (512..65535)')
    parser.add_argument('--frames', default='4096,256,64,16', help='cache-suite frame capacities, largest first')
    parser.add_argument('--allocations', action='store_true',
                        help='separate diagnostic run with process-wide allocation traffic; not RSS')
    parser.add_argument('--profile', action='store_true',
                        help='Linux gperftools measured-window CPU profile; requires one case/backend/trial')
    parser.add_argument('--profiler-library', default='/usr/lib/x86_64-linux-gnu/libprofiler.so.0',
                        help='gperftools library to preload for --profile')


def validate_options(parser, args):
    try:
        frames = [int(value) for value in args.frames.split(',')]
        tasks = [int(value) for value in args.tasks.split(',')]
    except ValueError:
        parser.error('--frames must be comma-separated integers')
    if not tasks or min(tasks) < 1 or max(tasks) > args.keys or len(set(tasks)) != len(tasks):
        parser.error('--tasks must be distinct counts between 1 and --keys')
    if args.task_frames < 8:
        parser.error('--task-frames must be at least 8')
    if not frames or min(frames) < 8 or len(set(frames)) != len(frames):
        parser.error('--frames must contain distinct capacities of at least 8')
    if args.keys <= 256 or args.key_bytes < 8 or not 512 <= args.page_size <= 65535:
        parser.error('require --keys >256, --key-bytes >=8 and --page-size 512..65535')
    if args.profile:
        if not sys.platform.startswith('linux') or not Path(args.profiler_library).is_file():
            parser.error('--profile requires Linux and an existing --profiler-library')
        if not args.case or args.backend == 'both' or args.repeats != 1:
            parser.error('--profile requires --case NAME --backend paged|baseline --repeats 1')
        if args.allocations or args.diagnostics:
            parser.error('collect CPU samples separately from --allocations/--diagnostics')


def configure_environment(env, args, output):
    # Clear inherited instrumentation so normal throughput runs stay uncontaminated.
    for key in ('MUDU_BENCH_SUITE', 'MUDU_BENCH_KEYS', 'MUDU_BENCH_KEY_BYTES',
                'MUDU_BENCH_PAGE_SIZE', 'MUDU_BENCH_FRAMES', 'MUDU_BENCH_ALLOCATIONS',
                'MUDU_BENCH_PROFILE', 'MUDU_BENCH_TASKS', 'MUDU_BENCH_TASK_FRAMES', 'CPUPROFILE'):
        env.pop(key, None)
    env['MUDU_BENCH_SUITE'] = args.suite
    if args.suite in ('cache', 'tasks'):
        env.update(MUDU_BENCH_KEYS=str(args.keys), MUDU_BENCH_KEY_BYTES=str(args.key_bytes),
                   MUDU_BENCH_PAGE_SIZE=str(args.page_size), MUDU_BENCH_FRAMES=args.frames,
                   MUDU_BENCH_TASKS=args.tasks, MUDU_BENCH_TASK_FRAMES=str(args.task_frames))
    if args.allocations:
        env['MUDU_BENCH_ALLOCATIONS'] = '1'
    if args.profile:
        env['LD_PRELOAD'] = str(Path(args.profiler_library).resolve()) + (
            ':' + env['LD_PRELOAD'] if env.get('LD_PRELOAD') else '')
        env['MUDU_BENCH_PROFILE'] = str(output / 'cpu.prof')
        env['CPUPROFILE_FREQUENCY'] = '997'


def profile_executable(root):
    """Build before preloading the profiler; retain the exact executable path."""
    cmd = ['cargo', 'test', '--release', '--manifest-path', 'crates/db-kernel/Cargo.toml',
           '-p', 'index', '--lib', '--features', 'bench-profile', '--no-run', '--message-format=json']
    result = subprocess.run(cmd, cwd=root, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(result.stderr + result.stdout)
    for line in result.stdout.splitlines():
        record = json.loads(line)
        if record.get('reason') == 'compiler-artifact' and record.get('executable'):
            return record['executable']
    raise RuntimeError('cargo did not report the benchmark executable')


def write_records(output, name, fields, rows):
    if rows:
        with (output / name).open('w', newline='') as file:
            writer = csv.DictWriter(file, fieldnames=fields)
            writer.writeheader()
            writer.writerows(rows)


def analyze_cache(rows, cache_rows, output):
    """Only a verified fully resident measurement can be a no-miss baseline."""
    cache = {(r['case'], r['backend'], r['trial']): r for r in cache_rows}
    groups = {}
    for row in rows:
        info = cache[(row['case'], row['backend'], row['trial'])]
        op = int(info['operations'])
        item = dict(row, pages=int(info['pages']), operations=op,
                    ns_op=float(row['seconds']) * 1e9 / op,
                    reads_op=float(row['reads']) / op,
                    evictions_op=float(row['evictions']) / op)
        groups.setdefault((row['case'], row['backend']), []).append(item)
    result = []
    for (_, backend), trials in groups.items():
        r = trials[0]
        item = {key: r[key] for key in ('case', 'workload', 'backend', 'keys', 'key_bytes', 'page_size', 'frames')}
        for key in ('pages', 'ns_op', 'reads_op', 'evictions_op', 'ops_s', 'p99_us'):
            item[key] = statistics.median(float(t[key]) for t in trials)
        item['resident'] = backend == 'paged' and all(
            int(t['frames']) >= t['pages'] and float(t['reads']) == 0 and float(t['evictions']) == 0 for t in trials)
        result.append(item)
    for row in result:
        bases = [r for r in result if r['workload'] == row['workload'] and r['resident']]
        base = max(bases, key=lambda r: int(r['frames'])) if bases else None
        row['baseline_case'] = base['case'] if base else None
        row['relative_ns'] = row['ns_op'] / base['ns_op'] if base else None
        row['extra_ns_op'] = row['ns_op'] - base['ns_op'] if base else None
    (output / 'cache_analysis.json').write_text(json.dumps(result, indent=2) + '\n')
    write_records(output, 'cache_analysis.csv', list(result[0]), result)
    lines = ['# Fixed-dataset cache capacity analysis', '',
             'Full warmup visits every key before measurement. Pages is the allocated page-ID span (next_page - 1), a conservative capacity bound including holes. Resident requires capacity >= that bound and zero measured reads/evictions in every trial.',
             'Reads/op are backing loads per logical operation, not a cache miss probability. A scan can load multiple pages.',
             'Extra ns/op is a descriptive difference from the largest verified resident budget; it includes cache management, copies, validation and scheduling, not just I/O.',
             'Deletion shrinks the dataset, so its cache pressure varies during the run. Compare matching workloads only.', '',
             '| Workload | Backend | Frames | Pages | Resident | ns/op | vs resident | Extra ns/op | Reads/op | Evictions/op |',
             '| --- | --- | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: |']
    for r in result:
        relative = f"{r['relative_ns']:.3f}x" if r['relative_ns'] is not None else 'unavailable'
        extra = f"{r['extra_ns_op']:+.0f}" if r['extra_ns_op'] is not None else 'unavailable'
        lines.append(f"| {r['workload']} | {r['backend']} | {r['frames']} | {r['pages']:.0f} | {r['resident']} | {r['ns_op']:.0f} | {relative} | {extra} | {r['reads_op']:.3f} | {r['evictions_op']:.3f} |")
    if not any(r['resident'] for r in result):
        lines += ['', 'No verified resident baseline: increase --frames and rerun. No baseline was inferred from a case name.']
    text = '\n'.join(lines) + '\n'
    (output / 'cache_analysis.md').write_text(text)
    with (output / 'summary.md').open('a') as file:
        file.write('\n' + text.replace('# Fixed-dataset', '## Fixed-dataset', 1))


def diagnostic_report(rows, cache_rows, diagnostics, allocations, output):
    """Normalize measured work; allocation traffic is process-wide, not live bytes."""
    by_trial = {(r['case'], r['backend'], r['trial']): r for r in rows}
    counts = {(r['case'], r['backend'], r['trial']): int(r['operations']) for r in cache_rows}
    groups = {}
    # Tree work counters exist only on paged; baseline DIAG zeroes are placeholders.
    for record in [r for r in diagnostics if r['backend'] == 'paged'] + allocations:
        key = record['case'], record['backend'], record['trial']
        row = by_trial[key]
        # Baseline matrix records lack exact op counts; timing/rate recovers the integer count.
        ops = counts.get(key, round(float(row['seconds']) * float(row['ops_s'])))
        sample = groups.setdefault(key, {})
        for field, value in record.items():
            if field not in ('case', 'backend', 'trial') and value != '':
                sample[field + '_per_op'] = int(value) / max(1, ops)
                if field in ('moved_bytes', 'alloc_bytes') and int(row['rows']):
                    sample[field + '_per_row'] = int(value) / int(row['rows'])
    normalized = [dict(case=k[0], backend=k[1], trial=k[2], **v) for k, v in groups.items()]
    (output / 'work_stats.json').write_text(json.dumps(normalized, indent=2) + '\n')
    lines = ['\n## Optional work statistics', '',
             'These runs are instrumented; use a separate uninstrumented run for throughput conclusions.',
             'Allocation calls include successful alloc/alloc_zeroed/realloc; bytes count requested sizes (full new size for realloc). Runtime/harness work inside the measurement window is included. Not RSS or live memory.', '',
             '| Case | Backend | Trial | Moved B/op | COW B/op | Alloc calls/op | Alloc B/op | Alloc B/row |',
             '| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |']
    for r in normalized:
        values = [f"{r[k]:.2f}" if k in r else '—' for k in ('moved_bytes_per_op', 'cow_bytes_per_op', 'alloc_calls_per_op', 'alloc_bytes_per_op', 'alloc_bytes_per_row')]
        lines.append(f"| {r['case']} | {r['backend']} | {r['trial']} | " + ' | '.join(values) + ' |')
    with (output / 'summary.md').open('a') as file:
        file.write('\n'.join(lines) + '\n')
