# Paged index performance matrix

The opt-in benchmark compares the current paged B-link tree with the original
`BTreeIndex`. Despite its historical name, the original implementation wraps
Rust's `BTreeMap`; it is not a separately implemented B+ tree. The benchmark
uses that actual implementation, not a substitute ordered map.

Previous results: five-optimization comparison (historical benchmark artifact; not tracked)
and extended verification (historical benchmark artifact; not tracked).
Earlier sections retain historical measurements.

Run from the repository root. No arguments selects the task suite (eight workloads at 1, 4 and 16 local tasks),
5,000 operations per task, three trials, both backends, and automatic PNG/SVG
plots. The default output directory is timestamped under `target/bench/` to keep
previous runs. Terminal output uses the shared Cargo-style status formatting:
completed trials show throughput and p99 latency, followed by a median comparison
table and artifact paths. Raw `BENCH`/`DIAG` records and routine test harness
lines are hidden by default but always retained in `raw.log`; pass `-v` or
`--verbose` to stream them. Compiler diagnostics and failures remain visible.
Colors automatically turn off for redirected output, `NO_COLOR`, or `TERM=dumb`.

Plotting requires matplotlib (`python3 -m pip install matplotlib`);
missing plotting dependencies are reported before building/running the benchmark.

```sh
# Show every argument, its default, and runnable examples:
python3 script/bench/paged_index.py --help
# Run with all defaults, including plots:
python3 script/bench/paged_index.py
# Explicit output and workload size:
python3 script/bench/paged_index.py --ops 5000 --repeats 3 \
  --output target/bench/paged-index
# One case for iteration:
python3 script/bench/paged_index.py --case mixed90-t4 --ops 20000 --repeats 5 \
  --output target/bench/paged-index-mixed90-t4
```

Use a native compiler for native dependencies. If the shell puts a WASI clang
first on PATH, prefix these commands with
`PATH=/usr/bin:/bin:$PATH CLANG_PATH=/usr/bin/clang`.
The runner builds release code before timing and saves `raw.log`, `results.csv`,
`summary.json`, `summary.md`, and `metadata.json`. Metadata includes the CPU,
OS, Rust compiler, Git revision/status, arguments, and hashes of the index and
buffer source files, so results from an uncommitted implementation remain
identifiable. Use a new output directory to retain previous runs. Temporary
backing files live on the output directory's filesystem and are removed at exit.

The Rust entry point is the ignored test
`paged::bench_matrix::performance_matrix`. Direct invocation accepts
`MUDU_BENCH_OPS`, `MUDU_BENCH_REPEATS`, and an optional exact
`MUDU_BENCH_CASE`; file cases also require an existing private
`MUDU_BENCH_DIR`. Normal test runs do not run this performance matrix.

## Partition ownership and integration

`PagedBTree` and `ClockBufferManager` are neither Send nor Sync. Frames, page
images and traversal leases use Rc and checked local borrows; page IDs, pin
counts, dirty generations, tree counters and residency hints use Cell. Fair
local wait queues retain cancellation safety across suspension without spinning
or atomic latch acquisition. I/O providers and optional diagnostic policies can
still use shared handles; immutable page snapshots retain COW for pending I/O.
The existing snapshot/revision checks remain necessary across suspension.

The default task benchmark accesses the local tree directly. Production relation
interfaces that still require Send futures use an owner mailbox: the owning
thread polls an operation immediately, retains a Pending operation in a local
task, and sends only owned results. Cross-thread callers submit messages, never
access tree bytes. Dropping a pending response aborts the owner operation before its next poll,
preserving relation-level cancellation ordering; submitted file writes retain the
existing OrderedFile completion discipline. Shared metadata catalogs use dedicated owner executors;
normal worker relations register on the worker's existing LocalSet. Runtime
shutdown cancels local tasks and releases registered trees. These message and
relation-layer costs are outside this index microbenchmark.

The old cross-thread frame Loom model has been replaced by tests of actual local
wait queues, cancellation/wakeup, immutable snapshots and owner-mailbox routing.
Compile-fail documentation tests reject Send/Sync trees and Send buffers.

## Partition-local task suite

### Measured partition-local results (2026-11-01, cached write descent, cached-leaf scans, write memo, patched splits)

The validation run used the defaults below: 4096 initial keys, 8-byte keys,
4096 frames, 20000 operations per task, three trials, release mode, and no
diagnostic instrumentation. Both backends ran on the same current-thread
executor in separate trials. Ratios below are median paged throughput divided
by median baseline throughput; 1.0 means equal throughput.

| Workload | 1 task | 4 tasks | 16 tasks |
| --- | ---: | ---: | ---: |
| Read | 1.433 | 1.431 | 1.416 |
| 90% read / 10% replacement | 1.397 | 1.339 | 1.345 |
| 50% read / 50% replacement | 1.184 | 1.201 | 1.193 |
| Changed-value update | 1.071 | 1.153 | 1.221 |
| Insert | 0.810 | 0.661 | 0.662 |
| Delete | 0.692 | 0.906 | 0.992 |
| Scan 32 | 0.871 | 0.880 | 0.877 |
| Scan 256 | 0.762 | 0.773 | 0.805 |

Reads, mixes and changed-value updates exceed baseline throughput, and deletes
reach parity under task pressure. The descent runs over a
write-epoch-validated decoded-node cache (bounded by the frame budget) on both
the read path and the resident write path; leaf writes patch the cached node
in place instead of forcing a re-decode; scans materialize rows straight from
the cached leaf arena; splits patch separator entries into parents that have
room instead of decoding and re-encoding every ancestor. Ordinary operations
take a single buffer traversal lease; quiescence-needing operations (exact
count, scans, checkpoint, reclamation) take the buffer's exclusive maintenance
lease. Ordered append/delete streams skip the descent entirely through a
single-leaf write memo armed only by boundary writes, so random and mixed
phases never pay for the probe. Narrowing the split structure lock to the
publication section alone did not move this benchmark: under cooperative
single-thread scheduling, lock waits do not idle the CPU, so only
per-operation work matters.

Inserts and long scans still have gaps; both are structural: the leaf patch
moves bytes within the packed page (inherent to the format; scans benefit from
that same layout), and the benchmark forces per-row materialization, so scan256
pays one key allocation per row on both sides plus page-boundary bookkeeping on
the paged side. Insert p99 fell from 63-68 us to 42-45 us with patched split
publication. More local tasks do not add CPU capacity.
Paged scan256 p99 rises from 21.20 us at one task to 18.89 us at 16 tasks,
while baseline changes from 14.22 us to 15.15 us. Paged scans cooperate between
pages; their measured operation latency includes intervening task execution,
whereas a baseline range completes synchronously. This distinction matters
when interpreting tail latency and should not be hidden by aggregate throughput.
The resident read/update/delete/scan measurements recorded zero backing reads
and evictions. Inserts still recorded backing reads/writes for newly created
pages, even without eviction; new-page admission remains an optimization target.

The local artifacts are `target/bench/e7/summary.md`,
`task_analysis.csv`, and `comparison.png` / `comparison.svg`. Raw results remain
under ignored `target/`. These are a new architecture baseline, not a measured
speedup over historical shared-thread results. The suite also excludes the
production relation mailbox overhead described above.

```sh
python3 script/bench/paged_index.py --suite tasks --tasks 1,4,16 --ops 20000 --repeats 3
python3 script/bench/paged_index.py --suite tasks --tasks 1,4,16 --task-frames 16 --ops 5000
python3 script/bench/paged_index.py --suite matrix --ops 5000
```

The task suite fixes 4096 keys, 8-byte keys, 4 KiB pages and 4096 frames by
default. Dataset options and --task-frames override these. Workloads are read,
mixed90, mixed50, changed-update, insert, delete, scan32 and scan256. Case names
include task count, e.g. read-t4. All suites now use local tasks on one thread;
the retained matrix changes data shape and cache settings, not thread count.
Each task has --ops operations, except deletes capped at keys/tasks. Inserts
use disjoint new-key ranges; changed updates use disjoint existing-key ranges.
The data volume therefore grows with task count for insert, so its curve is not
an isolated scheduler-overhead experiment. Task counts must not exceed keys.

Results use backend names baseline and paged, and a tasks column. The task suite
adds task_analysis.csv/json and plots aggregate throughput (trial min–max) and
p99 against local task counts. It does not claim multi-core speedup. Historical
executables/reports with the old runtime/backend schema must not be mixed into
this comparison. Run new results in a fresh directory.

## Comparison contract

Both backends receive identical precomputed deterministic traces per trial and
task, using a lexicographic byte comparator and `u128` values. This measures
index mechanisms, not production tuple-comparison costs. Backend order
alternates across trials. Keys and traces are built outside timing; insertion
still pays for the key clone required by the public API. Every operation is
checked for its expected result, and the entire final ordered mapping is
validated outside timing. A failed operation aborts the run rather than being
silently omitted from throughput.

The baseline tree is accessed directly by local tasks without an outer async
mutex. Its original internal synchronous lock remains part of the baseline.
Both indexes are confined to the same executor thread in separate matched trials.
The paged tree uses inline row IDs by default, matching the relation's index
representation; `generic-values` exercises its separate generic value directory.
Thus the primary comparison measures the current public index paths, rather
than an attempt to equalize their internal representations.

All cases use one Tokio current-thread runtime and a LocalSet, with one buffer
partition. The task suite defaults to 1, 4 and 16 local tasks; --tasks overrides
these levels. Tasks start at a barrier. More tasks do not add CPU cores. Both implementations yield after each
64 operations to avoid synchronous resident operations monopolizing a runtime
thread. Throughput includes barrier release, worker execution, periodic yields,
and collection of the worker results. Latency surrounds each index operation,
including lock waits, result validation, and async suspension; it excludes the
periodic yield. Every operation is timed, avoiding the old benchmark's periodic
sampling bias toward writes. This instrumentation costs time for both backends,
particularly the much faster baseline operations.

Each trial starts with a fresh index loaded by sequential insertion. A buffer
flush/checkpoint follows, then up to 512 deterministic point-read warmups.
`file-cold-start` instead reopens a checkpoint with a fresh buffer and skips
warmup; restore validates the root, so this is not literally a zero-resident-page
start. The OS page cache is not dropped. Only the operation region contributes
to throughput, latency, and cache-counter deltas. Load and preload-flush timings
are reported separately; the cold-start preload-flush measurement also includes
checkpoint reopen and root validation. Writes performed by the operation region are not
necessarily durable at its end.

## Axes

| Axis | Cases / settings |
| --- | --- |
| Operation | Hits, misses, 90/10 and 50/50 reads/replacements, replacements, new inserts, deletes |
| Range width | 32 and 256 returned rows per scan |
| Concurrency | 1 / 2 / 4 tasks for reads, 90/10 mix, and replacements |
| Dataset size | 1,024 / 4,096 / 32,768 starting keys |
| Key representation | 8 / 128 / 2,048 bytes; dispersed suffixes versus a common prefix for 128-byte keys |
| Contention | Uniform full-keyspace access versus a shared 16-key hot set with four tasks |
| Page size | 4 / 16 / 32 KiB at a constant 4 MiB frame payload budget |
| Buffer pressure | 16 / 64 / 256 frames with 4 KiB pages and 8,192 keys |
| Frame hints | Enabled versus disabled for the baseline read case |
| Scan prefetch | Enabled versus disabled for 256-row scans |
| Value representation | Inline row IDs versus the generic in-memory value directory |
| Backing store | MemoryPageStore and FilePageStore under pressure; reopened file cache |

The baseline is 4,096 keys, 8-byte keys, 16 KiB pages, 256 frames, hints and
prefetch enabled, one worker, inline values, and memory backing. Case definitions
live together in `bench_matrix.rs`. Deletes use disjoint keys and cap each
worker's operation count at `keys / tasks`; inserts use disjoint new keys.
Insert and delete traces are sequential within each worker; inserts append above
the initial maximum key. Miss probes are also above the maximum key, so they
measure right-edge absence rather than random holes inside the index.
Updates preserve each key's original value, allowing deterministic validation
under contention. Uniform and mixed traces use a deterministic xorshift PRNG;
this is not a Zipfian workload. Long keys have deterministic non-common suffixes;
the prefix case places the varying ID at the end of a common prefix.

## Reading the results

`results.csv` contains one row per backend, case, and trial. It records operations
per second, elapsed seconds, p50/p95/p99 microseconds, load and preload-flush
milliseconds, buffer reads/writes/evictions/hint hits, and rows processed.
For a scan, operations/s counts whole scans; returned rows/s is operations/s
times the scan width. A hint hit counts a successful buffer pin, so several
hits can occur per tree operation; it is not a query-level hit ratio.

`summary.md` reports median throughput, the min–max throughput range, and median
per-trial latency percentiles. These are descriptive repetitions, not confidence
intervals; a median p99 is not the p99 of pooled samples. Short baseline trials
are especially sensitive to scheduler noise, so increase `--ops` and repetitions
for hardware comparisons. There are no timing thresholds in CI.

Frame capacity is **not total memory consumption**. MemoryPageStore retains all
backing bytes, and snapshots, decoded nodes, values, traces, and output vectors
consume additional memory. This suite evaluates cache pressure via counters;
it does not measure peak RSS or establish a hard memory bound. File results
include the configured filesystem and OS cache, not direct-I/O device bandwidth.
No transaction/WAL, SQL execution, MVCC row materialization, crash recovery,
online maintenance interference, or open-loop saturation is measured. The
checkpoint measurement is a preload flush, not end-to-end relation checkpoint
latency. Those require separate system-level experiments.

Successful runs automatically print the report, CSV, PNG and SVG paths.
Use `--no-plot` for data-only runs, or `--backend paged` / `--backend baseline`
for one implementation; `--backend both` is the default. Small/single-case plots
show throughput (with min–max whiskers) and p99 latency. The full matrix shows
workload, scaling, key-size and cache-pressure panels. Missing axes in a
single-backend report are explicitly marked as unmeasured.

To regenerate a plot from an existing report without running the benchmark:

```sh
python3 script/bench/paged_index_plot.py target/bench/paged-index
```

This command also requires matplotlib and writes `comparison.svg` and
`comparison.png` from the saved summary, without rerunning measurements.

## Measured evaluation: 2026-10-31

Full report (historical benchmark artifact; not tracked),
raw trial CSV (historical benchmark artifact; not tracked),
environment/source metadata (historical benchmark artifact; not tracked).

The full 31-case matrix ran with 5,000 operations per task and three trials,
producing 186 successful backend/case/trial measurements. Deletes were capped
at 4,096 operations. The host was an AMD Ryzen 9 3900X (12 cores / 24 hardware
threads), Linux x86-64, Rust 1.99.0, release profile. The harness used four
runtime threads. No other builds/checks were launched by this task during the
measurements; the host was not isolated from other activity or CPU frequency
changes. The entire matrix, including setup and validation, took 112.39 seconds.

Index performance comparison (historical benchmark artifact; not tracked)

Selected medians:

| Workload | Baseline ops/s | Paged ops/s | Baseline p99 µs | Paged p99 µs |
| --- | ---: | ---: | ---: | ---: |
| Reads, 1 worker | 1,971,278 | 64,396 | 0.57 | 21.50 |
| Reads, 4 tasks | 1,188,283 | 209,509 | 6.54 | 27.70 |
| 90/10 read/replace, 1 worker | 1,795,375 | 41,324 | 1.00 | 97.30 |
| 90/10 read/replace, 4 tasks | 1,181,349 | 100,818 | 6.20 | 358.48 |
| Replacements, 1 worker | 1,103,360 | 9,434 | 1.06 | 118.66 |
| New inserts, 1 worker | 877,612 | 4,835 | 1.36 | 222.39 |
| 256-row scans, 1 worker | 103,572 | 15,423 | 14.54 | 90.42 |

The paged implementation was slower than the baseline wrapper in **every tested
case**. It is not ready to replace the default on performance grounds. Its
memory-residency and persistence capabilities are different from those of the
baseline map, but these do not explain away the large hot-cache throughput gap.

The measurements support the following observations:

- **Concurrency helps, but does not close the gap.** Paged reads improve 3.25×
  from one to four tasks; the 90/10 mix improves 2.44×. Four-worker replacement
  p99 rises to about 2.20 ms. Restricting four-worker mixed traffic to 16 keys
  lowers throughput from 100,818 to 54,215 ops/s and raises p99 from 358 to
  896 µs. These results expose contention costs without identifying their
  precise CPU or latch attribution.
- **Page size matters even with zero backing reads.** At the same 4 MiB frame
  payload budget, 4 / 16 / 32 KiB pages deliver 104,254 / 64,396 / 35,710 reads/s.
  This is consistent with per-access decoding costs increasing with page
  contents; it is not a CPU profile proving that explanation. Smaller pages
  may trade away fanout or I/O efficiency in other workloads.
- **Cache pressure is visible and controllable.** With 8,192 keys and 4 KiB
  pages, 16 / 64 / 256 frames deliver 63,059 / 72,046 / 96,471 reads/s. Median
  backing reads per 5,000 operations fall from 4,625 to 2,996 to zero. The
  16-frame file-backed case reaches 23,613 reads/s. The reopened file case
  reaches 25,983 reads/s, with overlapping trial ranges; this does not establish
  a benefit from starting cold, and neither case clears the OS cache.
- **Long keys need further investigation.** The 2,048-byte case reaches only
  4,706 reads/s, with about 31,171 backing reads per 5,000 lookups, even on
  MemoryPageStore. Overflow traversal and residency pressure are therefore
  important targets. The common-prefix 128-byte case is slower than the
  dispersed 128-byte case (46,687 versus 74,030 reads/s); this changes comparison
  work and page density together and is not an isolated compression on/off test.
- **Small optimizations do not remove the main regression.** Hint-enabled
  baseline reads have a higher median than hints-off (64,396 versus 54,324),
  but trial ranges overlap. Prefetch on/off scans also overlap, and the working
  set incurs no backing reads. There is no demonstrated prefetch benefit here.
  Generic values yield 63,591 reads/s versus 64,396 for inline IDs, so inline
  representation alone does not explain the read gap.
- **Write costs dominate before durability.** Replacements and new inserts are
  particularly slow, even though the timed region does not fsync each write.
  Existing full-page encoding/checksumming and snapshot allocation are plausible
  optimization targets. Sequential preload of 4,096 baseline keys takes about
  796 ms for paged; preload flush adds only about 0.04 ms with memory backing.

Prioritize profiling the hot read/decode and write/encode paths, then testing
byte-page views and allocation reduction against this matrix. Investigate
long-key overflow access separately. Evaluate genuine pointer swizzling and
optimistic latching with matched ablations after those costs are understood;
these results do not attribute the entire gap to synchronization. Preserve
correctness and cancellation tests when changing any of those protocols.

## P0/P1/P2 evaluation: 2026-10-31

This run measures the implementation after the first three optimization stages.
The earlier section and artifacts remain the **pre-optimization** baseline;
its statement that every case was slower applies to that earlier run only.

Full current report (historical benchmark artifact; not tracked),
all before/after comparisons (historical benchmark artifact; not tracked),
raw measurements (historical benchmark artifact; not tracked), and
environment/source metadata (historical benchmark artifact; not tracked).

The same 31 cases ran for three trials and 5,000 operations per task, with
identical trace generation and no timing/validation changes to the benchmark.
All 186 measurements and final mapping checks passed. The runtime, page settings,
value representation, and baseline mutex wrapper are unchanged. The host and
Rust toolchain are the same; separate runs are still subject to host noise.
No builds or tests were launched concurrently with the measured run. The full
matrix, including setup and validation, took 36.44 seconds.

| Workload | Previous paged ops/s | Optimized paged ops/s | Improvement | Current baseline ops/s |
| --- | ---: | ---: | ---: | ---: |
| Reads, 1 worker | 64,396 | 697,617 | 10.83× | 1,855,849 |
| Reads, 2 tasks | 121,973 | 1,196,570 | 9.81× | 1,320,097 |
| Reads, 4 tasks | 209,509 | 1,739,571 | 8.30× | 1,202,457 |
| 90/10 read/replace, 4 tasks | 100,818 | 881,795 | 8.75× | 1,121,612 |
| Replacements, 1 worker | 9,434 | 58,242 | 6.17× | 1,222,846 |
| New inserts, 1 worker | 4,835 | 16,915 | 3.50× | 898,697 |
| Deletes, 1 worker | 14,135 | 58,441 | 4.13× | 1,743,717 |
| 256-row scans | 15,423 | 36,178 | 2.35× | 101,752 |

Optimized index comparison (historical benchmark artifact; not tracked)

Four-worker reads now exceed the baseline relation-wrapper baseline by **1.45×**,
with median per-trial p99 of 3.73 µs versus 6.48 µs. Two-worker reads reach about
91% of the baseline, and four-worker 90/10 traffic reaches about 79%. This is
not a claim of universal parity: single-worker reads still reach only about
38% of baseline throughput, and write-heavy and overflow-key workloads remain
substantially slower. The baseline is the original tree behind its actual
relation-layer async mutex, not an unsafe concurrently shared bare BTreeMap.

The implementation changes are:

- **P0:** value-only lookup avoids materializing the stored key; inline paths
  bypass generic value-directory locks; point descent retains no ancestor Vec
  and returns the original leaf pin; replacements do not reserve new value IDs.
- **P1:** borrowed `NodeView` searches individual slots directly and also serves
  range/extreme reads. Short compressed keys use stack scratch with the original
  comparator, including its operand order. Complete external image validation
  covers unvisited slots and is memoized by immutable identity, never page ID
  alone. No decoded-node cache or format knowledge was added to BufferManager.
- **P2:** ordinary leaf insert/delete/replacement edits private page bytes;
  prefix changes or insufficient space fall back to recompression/splitting.
  Full-width inline values, revisions, fences, and MBP1 checksums are preserved.
  Generated images can be remembered at publication without repeating integrity
  work on the next access. Generic replacement only swaps the external value
  under its lock and a revision-validated shared page latch, since its page ID
  mapping does not change. Multi-page prepare/publish ordering remains intact.

P2 deliberately retains a private page copy for inline edits. A unique-owner
in-place mutation path was evaluated against the current immutable-image API:
writers already retain the source image, and older readers may retain it too.
Mutating the published allocation during preparation would violate snapshot or
cancellation guarantees. This implementation therefore keeps copy-before-publish
instead of expanding the buffer API to support transactional in-place byte edits.
It has **not** removed the full-page copy/checksum cost. The benchmark's inline
replacement cases still perform an actual page publication, even when the input
row ID equals the previous row ID; no benchmark-specific no-op shortcut was added.

The previous sensitivity to hot page size is much smaller: 4 / 16 / 32 KiB pages
now deliver about 744K / 698K / 714K reads/s, rather than 104K / 64K / 36K. This
supports the value of avoiding whole-node decode, though it is not a CPU profile.
The 2,048-byte key case improves only 1.24× to 5,819 reads/s, and file pressure
only 1.42× to 33,615 reads/s. Those paths still materialize overflow keys or pay
for backing I/O. Pointer swizzling, optimistic latch/epoch redesign, overflow
comparison shortcuts, and unique-owner in-place edits remain separate work.

Validation includes 47 index tests, 12 buffer tests (including the Loom reuse
model), the pin-lifetime compile-fail doctest, two Miri byte-view/patch tests,
and the paged-enabled server/storage/xcontract suites. New cases cover variable
record lengths, first/middle/last byte edits, retained snapshots, cancelled
publication, exhausted value IDs, inline operation independence from value locks,
comparator-equivalent stored keys, malformed unvisited slots with valid checksums,
and checkpoint restore after mixed variable-key edits. There are no timing gates.

## Five further optimizations: 2026-10-31

The follow-up implementation adds:

1. Deferred MBP1 checksums: an index-owned writeback policy seals a private
   persistent image at initialization/flush, rather than hashing every leaf edit.
2. Unique-owner byte edits: a checked `PageEdit` program uses `Arc::make_mut`
   under the exclusive page latch. The search snapshot is dropped before
   preparation. Shared readers/flushes retain immutable COW snapshots; abandoned
   preparation does not publish bytes or advance dirty epochs.
3. Resident lease/latch fast paths: nonblocking `try_access`, `try_read`, and
   `try_write` attempts precede asynchronous waits. Alternate managers can keep
   the default asynchronous lease implementation. Queued maintenance is not
   bypassed.
4. Per-image validation certificates: an atomic static policy identity replaces
   the lock-protected weak-image memo. Raw loads still validate checksum and every
   slot; every access separately checks page size and page ID. Byte edits clear
   or replace the certificate.
5. Split sizing summaries plus explicit lexical comparison: split candidate
   sizing preserves the old balance/tie policy without materializing every
   candidate. `with_lexicographic_keys()` enables compressed-head shortcuts and
   chunk-streaming comparisons for unsigned-byte comparators only. SQL tuple
   comparator behavior remains the default.

The original 31 cases retain their configurations, including the generic tuple
comparator path in `key-bytes-2048`. Four extra cases are reported separately:
`lexical-8`, `lexical-2048`, and `changed-update-t1`/`changed-update-t4`. Lexical
cases explicitly opt into the byte-order capability on the same generated keys.
Changed-update tasks own disjoint key ranges and write a different full-width
u128 value on every operation. Each previous value and the complete final mapping
are checked. The original update cases still replace values with themselves, but
neither backend has a same-value write shortcut.

The optimization moves checksum work to writeback; update throughput is not a
per-operation durability measurement. A bounded cache under sustained dirty-page
pressure still pays encoding, checksum, and I/O costs. Shared snapshots still
require page copies. MemoryPageStore retains backing bytes outside the frame
budget; frame capacity is not RSS. The feature remains opt-in.

### Measurements and remaining gap

The new full report (historical benchmark artifact; not tracked),
before/after table (historical benchmark artifact; not tracked),
CSV (historical benchmark artifact; not tracked), and
metadata/source hashes (historical benchmark artifact; not tracked)
cover 35 cases × two backends × three trials, with 5,000 operations per task.
This session ran no concurrent build/test commands during the measurements. Host-wide CPU
isolation was not used. Comparisons with the earlier P0/P1/P2 artifact are
historical; the original backend was rerun alongside the new implementation.

| Case | P0/P1/P2 paged ops/s | New paged ops/s | Improvement | Current original ops/s |
| --- | ---: | ---: | ---: | ---: |
| Read, one worker | 697,617 | 820,241 | 1.18× | 1,830,707 |
| Update, one worker | 58,242 | 744,322 | 12.78× | 1,139,469 |
| Insert | 16,915 | 393,500 | 23.26× | 772,830 |
| Delete | 58,441 | 576,781 | 9.87× | 1,740,302 |
| Read, four tasks | 1,739,571 | 1,886,850 | 1.08× | 1,243,789 |
| 90/10 mix, four tasks | 881,795 | 1,550,491 | 1.76× | 1,188,962 |
| Update, four tasks | 151,666 | 955,780 | 6.30× | 672,986 |

Workload, concurrency, key size and cache pressure comparisons (historical benchmark artifact; not tracked)

The default 2,048-byte comparator case improves from 5,819 to 7,571 ops/s.
Explicit lexical comparison reaches 31,378 ops/s, **4.14×** the current default
comparator path, but only **2.2%** of original. It still needs the matching key's
overflow data, and the fixed frame budget cannot retain all overflow pages.
This workload therefore remains far from parity. Scans and file-backed pressure
also improve little: scan256 is 37,315 versus 104,488 original ops/s; file-pressure
is 35,323 versus 1,924,820. These compare paged eviction/I/O with a fully resident
original map, not equivalent durable storage implementations.

Four additional extended runs (historical benchmark artifact; not tracked)
use **100,000 operations per task and five trials**:

| Case | Paged ops/s | Original ops/s | Paged/original |
| --- | ---: | ---: | ---: |
| Read, one worker | 854,828 | 1,964,974 | 0.435 |
| Read, four tasks | 1,873,406 | 1,214,912 | 1.542 |
| Changed-value update, one worker | 780,677 | 1,160,644 | 0.673 |
| Changed-value update, four tasks | 1,561,585 | 669,951 | 2.331 |

Changed-value tasks use disjoint key ranges; their result must not be presented
as the same contention pattern as the original shared-range update-t4 case.
Shared-range update-t4 throughput wins but its p99 is **69.28 µs**, versus
**9.22 µs** original. Hot-key traffic also remains slower than original. The
optimizations do not establish universal throughput or tail-latency parity.

Validation: db-kernel workspace Clippy in default and `server/paged-index`
configurations; workspace test-target compilation; 357 selected unit tests
(index 52, page_buffer 16, server 59, storage 84, xcontract 146); the pin-lifetime
compile-fail doctest; six Miri tests (four buffer edit/policy tests and two byte
view/patch tests); formatting and repository code-metrics checks. Tests cover
in-place byte-allocation reuse, COW snapshots, cancelled/invalid edits, weak-token
dissociation, failed writeback retry, dirty epochs during an older flush, fair
maintenance entry, raw checksum corruption, checkpoint restore, exact split-policy
parity, and lexical overflow streaming under eviction. Both benchmark suites
check results and the final complete mapping.

## Resident search, scan and contention counters

The optional diagnostic CSV also contains `latch_waits` and `latch_wait_ns`.
These measure resident page-latch attempts that could not complete immediately
and elapsed time for completed waits, respectively. Cancelled waits contribute
an attempt but no duration. Time includes scheduling delay; it is not CPU time.
Both clock reads and counter updates are disabled without `--diagnostics`.
Retained older executables remain supported: the runner leaves these columns
empty when their seven-field diagnostic records do not supply the new counters.
Compare throughput with diagnostics disabled, and collect counters separately.

`latch_probe_hits` counts contended acquisitions completed by the bounded
nonblocking probes before async queueing. `latch_waits` includes those attempts;
`latch_wait_ns` includes their probe time. All three counters remain disabled
without `--diagnostics`. The runner accepts both seven-field and nine-field
records from retained executables, leaving unavailable columns empty.

## Checksum and writeback diagnostics

`--diagnostics` adds four per-tree counters to `diagnostics.csv`:

| Column | Meaning |
| --- | --- |
| `checksum_calls` | Completed checksum computations during validation or writeback, including mismatches and writeback retries. |
| `checksum_bytes` | Bytes hashed, excluding the eight-byte checksum field but including free space. For fixed-size pages: `checksum_calls * (page_size - 8)`. |
| `checksum_ns` | Summed elapsed nanoseconds around hashing only; excludes copying, layout validation and counter updates. Includes scheduling delay and timer overhead, so it is not CPU time. |
| `writeback_copy_bytes` | Bytes copied into the index encoder's private writeback image. Excludes COW, packed-page movement and any subsequent backing-store copies. |

For example:

```sh
python3 script/bench/paged_index.py --case key-bytes-2048 --backend paged --diagnostics --no-plot
```

CSV values are differences over the measured operation interval; preload,
initial flush and final validation are excluded. Resident certificate hits and
header/identity rejections before hashing do not count. A clean repeated flush
performs no encoding. Failed backing writes still count the encoding work,
and retrying a write counts again. Concurrent snapshots are approximate until
tasks and writebacks have completed.

The API exposes the same cumulative fields through `performance_stats()` after
`with_diagnostics()`. Activation is idempotent and starts counting future work,
including deferred writes of images created before activation; initialization
work performed before activation is excluded. A shared index-owned writeback
policy carries attribution into buffer-managed flushes and retained snapshots.
BufferManager remains non-generic and does not know checksum formats or counters.

Without diagnostics there are no checksum clock reads or counter updates.
Collect throughput without diagnostics and counters in a separate run. The
runner accepts retained seven-, nine- and ten-field diagnostic records and
leaves unavailable columns empty. Instrumentation does not change checksum
semantics; the subsequent algorithm replacement is documented below.


## CRC32C page checksum

Page checksums now use `crc32c` 0.6.8, with runtime hardware dispatch and a
software fallback ([library documentation](https://docs.rs/crc32c/0.6.8/crc32c/)).
Magic remains MBP1 and the header remains 56 bytes. The existing eight-byte
checksum field at offset 40 stores the little-endian CRC32C zero-extended to
64 bits; validation compares all eight bytes, including the zero upper half.
The effective checksum width is 32 bits. Hashing covers `[0..40]` followed by
`[48..]`, including free space, using incremental CRC calls without a temporary
concatenation buffer. Resident certification and deferred writeback are unchanged.

There is no version bump, migration or FNV fallback. Old FNV index pages and
checkpoints are unsupported and must be rebuilt from authoritative data. The
row-directory and manifest checksum algorithms are not changed by this refactor.
Historical benchmark results above describe the earlier algorithm.

Regenerate the fixed leaf fixture with:

```sh
python3 script/bench/paged_index_fixture.py
```

The independent bitwise generator preserves every byte outside the checksum
field. Rust tests compare the production dispatch against a bitwise Castagnoli
oracle at page sizes 512, 4096, 16384, 32768 and 65535, verify the standard
`123456789` vector, and reject single-bit corruption at every byte of a page,
including the upper half of the checksum field.

## CRC32C measured comparison: 2026-10-31

Three alternating before/after rounds, one trial per round, paged backend only,
diagnostics disabled. Resident read and changed-update-t4 use 500,000 operations
per task; other cases use 20,000. Values below are throughput medians. Builds
and tests completed before measurement. This host supports SSE4.2; these results
do not characterize software-fallback performance.

| Case | FNV ops/s | CRC32C ops/s | Change |
| --- | ---: | ---: | ---: |
| read | 1,247,282 | 1,256,885 | +0.8% |
| changed-update-t4 | 1,978,917 | 1,956,574 | -1.1% |
| insert | 483,234 | 520,478 | +7.7% |
| key-bytes-2048 | 28,529 | 93,058 | +226.2% |
| frames-16 | 151,446 | 287,058 | +89.5% |
| file-pressure | 34,197 | 37,946 | +11.0% |

The small resident-path changes are observed deltas, not proof of improvement
or regression; this three-round run does not establish statistical significance.
A separate 5,000-operation diagnostic run for key-bytes-2048 reports 9,063
checksum calls and 148,415,688 bytes for both algorithms, with checksum time
falling from 146.339 ms to 23.271 ms (6.29x). Writeback copy traffic is zero in
that read-only case. Insert checksum time falls from 0.736 ms to 0.120 ms, with
46 calls, 753,296 checksum bytes and 376,832 writeback-copy bytes unchanged.

Local raw results and executable provenance are under
`target/bench/paged-crc32c/` and are not committed. The baseline executable was
built from commit `31a8152e`; the candidate contains this CRC32C refactor.


## Fixed-dataset cache capacity experiment

Use `--suite cache` to keep keys, key width, page size, task count, preload,
and deterministic per-trial operation traces fixed while changing only the
frame budget. The suite includes read, scan32, scan256 and delete; it uses a
memory backing store to isolate application-cache pressure from file I/O.
Default budgets are 4096, 256, 64 and 16 frames with 4096 keys and 4 KiB pages.
Every key is read during warmup, outside timing. Each trial starts from a newly
built tree; deletion shrinks that tree during measurement.

```sh
# Normal throughput: no diagnostics, allocator counters or CPU sampling.
python3 script/bench/paged_index.py --suite cache --ops 20000 --repeats 3
# Long keys: keep this separate dataset fixed across all capacities.
python3 script/bench/paged_index.py --suite cache --key-bytes 2048 --page-size 16384 --frames 4096,1024,256,64 --ops 20000
# Separate run for moved/COW/checksum bytes and allocation traffic, both backends.
python3 script/bench/paged_index.py --suite cache --diagnostics --allocations --ops 20000
```

`cache_analysis.md`, `.csv` and `.json` report ns/op, backing reads/op,
evictions/op, the verified resident baseline, and extra ns/op relative to that
baseline. The same table is appended to `summary.md`; `comparison.png` and
`comparison.svg` show capacity/throughput and capacity/backing-load curves.
`cache_samples.csv` records exact operation counts and the initial allocated
page-ID span (`next_page - 1`, a conservative upper bound that includes holes).
A paged case is marked resident only if its capacity covers that span and every
trial has zero measured backing reads and evictions. If no such baseline exists,
relative costs remain unavailable; increase `--frames` rather than assuming the
largest budget fits. Reads/op is not a miss probability: one logical operation,
especially a scan, may load multiple pages. Extra ns/op includes cache bookkeeping,
copying, validation and scheduling, not a direct measurement of device latency.
Frame capacity does not measure RSS, and MemoryPageStore retains backing data in RAM.

`--allocations` builds the test-only `bench-alloc` feature. `allocations.csv`
records successful allocation/reallocation calls and requested bytes during the
benchmark operation window, including runtime/task/harness traffic. Reallocation
counts its full new requested size; these are cumulative traffic counters, not
live or peak memory. The normal test/production build has no counting allocator.
`work_stats.json` and the summary normalize diagnostics per operation, with moved
and allocation bytes also available per returned row. Original and paged indexes
are both measured; do not attribute all scan allocations to paged-index alone.
Moved bytes count instrumented page mutations, not every memory copy: a read-only
scan can report zero while still copying keys into owned results. Baseline has no
page-mutation counters; its work table uses unavailable rather than fabricated
zeroes. Allocation traffic captures result allocation sizes, not memcpy volume.
Retained executables must be built with the matching instrumentation feature.

CPU sampling is a third, separate run (Linux with gperftools `libprofiler`):

```sh
python3 script/bench/paged_index.py --suite cache --case cache-scan256-f4096 --backend paged --repeats 1 --ops 100000 --profile
# Delete is capped by dataset size: increase keys, not only ops, for more samples.
python3 script/bench/paged_index.py --suite cache --case cache-delete-f4096 --keys 100000 --backend paged --repeats 1 --ops 100000 --profile
```

`--profile` builds `bench-profile`, preloads `--profiler-library`, and starts/stops
sampling around the operation measurement. Preload, warmup, final mapping
validation and teardown are excluded; task creation/joining and latency-sample
processing inside the window are included. `cpu.prof`, `cpu.json` and `cpu.txt`
are retained, with symbol summaries embedded in `summary.md`. Sample shares are
CPU attribution estimates, not wall-clock percentages. Use an exact retained
executable for symbolization, and increase workload duration if there are too
few samples. Profiling requires one case/backend/trial and cannot be combined
with allocation or diagnostic counters. Instrumented throughput must not be
used as the normal performance baseline.

The comparison does not yet implement new-page residency, optimistic traversal,
or delayed delete compaction; it supplies evidence for those separate changes.

### Local validation sample (2026-10-31)

The short-key cache suite was run without instrumentation after compilation,
using 4096 keys, 8-byte keys, 4 KiB pages, 20000 requested operations and three
trials per backend/capacity (96 measurements). Delete performs 4096 operations
because it exhausts the dataset. The allocated page-ID span was 77; both 256
and 4096 frames had zero measured reads and evictions in all trials.

| Workload | Baseline at 4096 frames, ns/op | Paged resident at 4096 frames, ns/op | Paged at 16 frames, ns/op | Extra vs resident, ns/op | Reads/op at 16 frames |
| --- | ---: | ---: | ---: | ---: | ---: |
| read | 530 | 785 | 2893 | 2108 | 0.801 |
| scan32 | 1738 | 4090 | 6743 | 2653 | 1.283 |
| scan256 | 9884 | 21130 | 36927 | 15796 | 4.801 |
| delete | 512 | 1082 | 1206 | 124 | 0.019 |

These descriptive medians expose a resident-path gap before cache pressure is
introduced. Sequential deletion has strong locality and a shrinking working set;
its small cache penalty must not be generalized to random deletion. The baseline
frame label only identifies the matched trial group; baseline has no frame budget.

A separate 1000-operation diagnostic/allocation run measured approximately
141 allocated bytes per returned row for paged scan256 versus 104 for baseline,
and 883 page-mutation bytes moved per paged delete at 4096 frames. These counters
motivate examining wide-scan result construction and delete compaction; they do
not establish an optimization's speedup. CPU profiles were collected separately
for both implementations' scan256 and delete, excluding setup/warmup. The short
baseline delete profile had only 63 samples and triggers the low-sample warning;
collect a larger dataset before assigning precise hotspot shares. The lightweight
symbolizer groups shared-library frames by library, so libc samples alone do not
distinguish allocator work from memcpy/memmove.

Local raw reports, plots and profiles are retained under
`target/bench/paged-cache-validation/` and are not committed. This validation
covers the default short-key dataset; long-key and larger-dataset conclusions
require their own fixed-dataset runs with the commands above.
