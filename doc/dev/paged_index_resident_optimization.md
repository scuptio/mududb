# Resident-path optimization and CPU evidence

This is historical CPU evidence from the shared-tree runtime. The current
partition-local implementation and single-thread task benchmark are documented
in [paged_index_benchmark.md](paged_index_benchmark.md); these older performance
ratios must not be treated as measurements of the new execution model.

This iteration implements the requested order **2 → 1 → 3 → 4 → 5 → 6**,
following CPU sampling and an instrumented baseline. BufferManager remains
non-generic and object-safe. No blocking mutex or unsafe mutable-page reads were
introduced. That iteration preserved MBP1 encoding, the checksum algorithm
and checkpoint manifests. The later CRC32C refactor replaces the page checksum
without changing magic, offsets or field widths; its measurements must be kept
separate from the historical results below.

## Measurement method

The host has `perf_event_paranoid=4`; `perf record -e cpu-clock:u` failed both
inside and outside the sandbox. No system security setting was changed.
The installed `libprofiler.so.0` supplies gperftools user-space CPU sampling at
997 Hz. `script/bench/paged_index_profile.py` reads its Linux x86-64 format and
resolves optimized executable symbols with `nm`. Shared-library samples are
reported by library, not attributed to individual libc functions. Inclusive
shares overlap; inlined code is attributed to its enclosing optimized symbol.
Profiles include setup, assertions, latency instrumentation, final validation and
teardown, and are not exact timings of individual index functions.

A retained executable from immediately after adding diagnostics, before the six
optimizations, is used for the baseline. Its benchmark generates only the key
domain needed by a workload, so long read/update runs no longer allocate unused
keys proportional to the operation count. Timed traces and correctness checks
remain the same. `--executable` records the retained binary's SHA256 and does not
claim that the current source hashes describe that older binary.

Throughput runs disable diagnostics and sampling. Separate diagnostic/profile
runs enable counters. Each benchmark still checks operation results and the
complete final mapping. The development host is not CPU-isolated.

Example sampling command (use the release test executable printed by Cargo):

```sh
LD_PRELOAD=/usr/lib/x86_64-linux-gnu/libprofiler.so.0 \
CPUPROFILE=target/read.prof CPUPROFILE_FREQUENCY=997 \
MUDU_BENCH_BACKEND=paged MUDU_BENCH_DIAGNOSTICS=1 \
MUDU_BENCH_CASE=read MUDU_BENCH_OPS=2000000 MUDU_BENCH_REPEATS=1 \
  target/release/deps/index-<hash> paged::bench_matrix::performance_matrix \
  --exact --ignored --nocapture --test-threads=1
python3 script/bench/paged_index_profile.py target/read.prof \
  target/release/deps/index-<hash>
```

Keep the exact executable that produced a profile. Do not symbolize an older
profile against a rebuilt executable. Original samples, symbol summaries and
binary hashes accompany the benchmark evidence.

## Optional counters

`PagedBTree::with_diagnostics()` enables per-tree counters;
`performance_stats()` returns cumulative values. Without this builder, the
additional counter RMWs are disabled. Counter snapshots during concurrent work
are approximate; quiescent before/after deltas are used in reports.

| Counter | Meaning |
| --- | --- |
| `retries` | Point-operation revision conflicts and split-latch conflicts requiring retry; initial resident fallback and maintenance are excluded |
| `cow_bytes` | Actual page-payload bytes copied while preparing published byte edits; weak-only identity dissociation copies zero bytes |
| `moved_bytes` | Published packed-page `copy_within` traffic; excludes field writes, serialization, writeback copies and temporary buffer construction |
| `overflow_reads` | Logical overflow page visits, including buffer hits; physical I/O remains in BufferStats |

Use `python3 script/bench/paged_index.py --diagnostics ...` to retain
`diagnostics.csv`. `--backend paged` isolates one backend for profiling;
`--executable <path>` runs a retained baseline. Detailed instrumentation can
perturb concurrency and must not be mixed with uninstrumented headline rates.

## Changes and boundaries

1. **Scan positioning (requested item 2).** Retain the descent image and exact
   lower slot. Locate the upper slot before constructing result keys. Subsequent
   pages start at slot zero without repeated lower-bound tests. Reserve result
   capacity per page and prefetch only after proving the next page is needed.
   Consistent-scan gating and inclusive/exclusive bounds are preserved.
2. **Resident lookup (item 1).** Try pin/read and search under a short shared
   latch; return inline values directly without cloning an image or reacquiring
   the leaf latch for revision validation. Contention, missing pages and keys
   requiring materialization return to the asynchronous path. No latch spans an
   await, backing I/O, or acquisition of another page latch.
3. **Inline comparison (item 3).** Keep ordinary short-key binary comparisons
   synchronous. Explicit lexical mode compares prefix/suffix slices without
   constructing a complete key and can reject a shared prefix once per node.
   Generic comparators preserve operand order and propagate errors; arbitrary SQL
   tuple encodings do not acquire byte-order shortcuts automatically.
4. **Resident leaf edits (item 4).** Recheck leaf type/fence after acquiring the
   write latch, search and prepare inline insert/update/delete without retaining
   a search snapshot. Split, recompression, overflow and contention use the
   existing async path. Actual concurrent snapshots still trigger COW.
5. **Long-key occupancy (item 5).** The inline head budget is now
   `(page_size - 192) / 4`, capped by actual key length, for generic comparators.
   Explicit lexical mode retains the additional 512-byte limit: it can reject
   nonmatching heads without fetching the overflow key, and larger heads reduced
   fanout/cache efficiency in the first measured candidate. A generic 2 KiB key
   fits inline on a 16 KiB page; small pages
   retain conservative overflow behavior. The descriptor already stores variable
   head length, so older overflow representations and new inline representations
   coexist without migration. This trades some fanout for fewer overflow pages;
   cache capacity and key size must still be evaluated together. Shared overflow
   packing is not introduced in this iteration.
6. **Edit allocations (item 6).** `PageEdit::write_owned` transfers prepared
   directory/record vectors instead of copying them again. A single ten-byte
   revision/count update keeps insert/delete programs within four inline edits,
   avoiding an extra program-vector allocation. Packed-page movement remains;
   page holes and deferred compaction would change layout validation and are not
   part of this format-compatible iteration.

The initial profiles identified descent/comparison/latch work for point reads,
`KeyView::owned` and repeated entry parsing in short scans, and checksum validation
for overflow-heavy 2 KiB keys. In the instrumented pre-change run, 800,000
shared-range updates encountered 43,764 retries and copied 270,483,456 page bytes;
20,000 long-key reads visited 265,306 overflow pages. These are observations from
one diagnostic run, not deterministic workload constants or throughput results.

## Results

Artifacts: same-host comparison (historical benchmark artifact; not tracked),
final full matrix (historical benchmark artifact; not tracked),
diagnostic evidence (historical benchmark artifact; not tracked), and
profile manifest (historical benchmark artifact; not tracked).
The full matrix retains 35 cases × two backends × three trials for both baseline
and final code. Five additional paired cases use 100,000 operations per worker
and five trials. Diagnostics and sampling are disabled for these rates.

| Extended workload | Before paged ops/s | Final paged ops/s | Improvement | Current original ops/s | Paged/original |
| --- | ---: | ---: | ---: | ---: | ---: |
| Read, one worker | 840,487 | 1,178,968 | 1.40× | 2,062,920 | 0.572 |
| Read, four workers | 1,932,296 | 2,593,074 | 1.34× | 1,225,441 | 2.116 |
| Changed update, one worker | 698,282 | 990,713 | 1.42× | 1,201,030 | 0.825 |
| Shared-range update, four workers | 954,192 | 1,100,909 | 1.15× | 649,231 | 1.696 |
| Disjoint changed update, four workers | 1,678,027 | 2,398,261 | 1.43× | 717,462 | 3.343 |

In the 5,000-operation matrix, scan32 improves **2.16×**, scan256 **1.18×**,
sequential insertion **1.15×**, deletion **1.24×**, and generic-comparator 2 KiB
key lookup **3.80×**. Single-thread read still reaches only about 57–61% of
original; changed-value update reaches about 82%. The earlier aspirational 70%
read/90% update targets were not reached. Scans and capacity-pressure workloads
also remain behind original.

Short four-worker read samples gave a regression (0.881×), while longer paired
runs gave 1.342×. Both observations, raw CSVs and min–max ranges are retained;
this is not evidence that every short burst or scheduling arrangement improves.
Shared-range update p99 improves from 70.59 to 58.58 µs in the extended run, but
remains far above original's roughly 9 µs. Disjoint updates must not be used to
claim the same result under shared-leaf contention.

The first candidate applied the larger inline-head budget to lexical keys too.
Its lexical 2 KiB case regressed from 33,878 to 28,515 ops/s, with reads rising
from 7,996 to 9,071. That candidate is retained in
candidate-large-head (historical benchmark artifact; not tracked).
Keeping lexical heads short restores reads to 7,996 and yields 33,119 ops/s,
roughly baseline throughput. Generic comparison benefits from avoiding repeated
full-key overflow reconstruction: backing reads fall from 31,185 to 9,071 in
the full matrix, although the remaining node working set still exceeds the
configured frame budget.

Final workload, concurrency, key-size and cache-pressure results (historical benchmark artifact; not tracked)

## What the counters and profiles establish

Matched diagnostic/profile runs use 800,000 operations for shared-range update
and 20,000 for each long-key case:

- Update retries fall from **44,576 to 24,904** (about 44% fewer); byte-edit COW
  falls from **284,884,992 to 17,907,712 bytes** (about 94% fewer). These quantities
  depend on scheduling and instrumentation; they are not deterministic constants.
- Generic 2 KiB lookups fall from **265,306 logical overflow visits to zero**.
  Lexical lookups retain **21,342** visits, consistent with the retained short
  head policy. Zero overflow visits does not imply zero backing reads.
- The separate full diagnostic matrix records **14,849,070 moved bytes** for
  deletion in both versions. Sequential insertion moves zero packed payload
  bytes in both. Item 6 reduces temporary copies/allocations, not the remaining
  packed-page memmove volume. Random-insert throughput is not measured by this
  sequential-insert case.
- Whole-process scan32 CPU samples fall from **1,861 to 854**; `KeyView::owned`
  falls from a prominent pre-change symbol to a small fraction of the final
  sample set. This supports removing unnecessary key construction, but does not
  isolate the contribution of every optimization.
- Generic long-key CPU samples fall from **3,487 to 892**, yet checksum code
  still accounts for about **74%** of final samples. Fewer page loads helped;
  cold-page validation remains dominant for this capacity-constrained case.
- Final point-read samples still show `NodeView::entry`/`slot`, synchronous node
  search/comparison and semaphore bookkeeping. The new path removes a snapshot
  clone/revalidation round; it does not eliminate safe page pinning/latching.

Optimized identical-code folding can report a representative generic symbol
with a different type parameter from the actual u128 benchmark. Detailed symbol
reports explicitly retain this limitation. All profiles include harness/setup
work; sample counts are supporting CPU evidence, not exact per-function timers.

## Validation

Workspace Clippy passed in default and `server/paged-index` configurations;
all workspace test targets compiled. Selected tests cover 57 index, 16
page_buffer, 59 server, 84 storage and 146 xcontract unit tests (362 total), plus
the pin-lifetime compile-fail doctest. Five targeted Miri tests cover snapshot
ownership, old overflow readability, new long-key checkpoint restore, scan
bounds and comparator-specific head budgets. Formatting, Python syntax and
repository code-metrics checks also pass. All benchmark traces and complete
final mappings are checked, including the retained-baseline runs.

## Follow-up: search decoding, batched scans and resident latch waits

This follow-up implements the next three selected optimizations without changing
MBP1 bytes, comparator semantics, the non-generic BufferManager interface, or
consistent-scan gating. New and updated source comments are in English.

- NodeView exposes separate checked key and value accessors. Binary search reads
  only keys; descent and leaf replacement decode only the selected value. Scans
  still decode both from a single checked record. Packed-edit tests compare both
  accessors with the independent full decoder, including compressed keys and
  nonzero upper value bits.
- Inline-value scans append each consecutive run of inline keys synchronously.
  Only overflow reconstruction enters the async materialization path. Returned
  keys remain owned, order and inclusive/exclusive bounds are unchanged, and the
  exclusive scan gate remains held for consistent mapping results. Generic
  values continue to use their value-directory locks.
- Resident point reads and inline writes wait asynchronously on the already
  pinned page when its latch is contended. This avoids restarting descent and
  retaining a speculative snapshot solely because a latch was unavailable.
  No page guard spans an await or acquisition of another page guard. A leaf
  write rechecks page type, fence and search position after acquiring its latch;
  a moved key or unsupported layout falls back to the existing structural path.
  Cancellation before acquiring the latch publishes nothing and releases the
  queued waiter and pin through normal RAII.

Optional `latch_waits` / `latch_wait_ns` counters distinguish contended resident
latches from revision-conflict retries. They are disabled in throughput runs,
including all extra clock reads. Wait time includes scheduler delay; cancelled
waits count as attempts but do not contribute a completed duration. The Python
runner accepts older diagnostic records and leaves their new columns empty.

Regression coverage adds deterministic cancelled-writer and reader-wakeup tests,
plus a bounded-cache scan mixing compressed inline and overflow keys. Existing
concurrent split/merge/reclamation, comparator-error, checkpoint, and generic
value tests remain part of validation.

Validation for this follow-up: workspace Clippy passed with default features
and with `server/paged-index`; all workspace test targets compiled. The index,
page_buffer, server, storage and xcontract library suites passed 354 tests with
default features and 365 with `server/paged-index`. The retained final release
executable also passed all 40 paged-specific tests (two benchmarks ignored).
Code metrics checked 1,741 files without violations.

Follow-up measurements and raw evidence:
paired comparison (historical benchmark artifact; not tracked),
full matrix (historical benchmark artifact; not tracked), and
plots (historical benchmark artifact; not tracked).
The paired runs use 100,000 operations per worker and five trials, alternating
before/after version order by case, after all builds and functional tests finish.

Single-worker read improves 10.5% to 1.287M ops/s (63.8% of original), scan32
improves 23.4%, scan256 31.4%, and changed-value single-worker updates 9.4%.
Hot mixed90 improves 28.4%; its median per-trial p99 falls from 56.53 to
17.92 us (68.3% lower). Shared-range update throughput improves only 1.2%,
with p99 falling from 57.96 to 47.16 us. Four-worker reads improve 3.2%.

The tradeoff remains visible: disjoint changed-value updates regress 6.6% in
throughput, from 2.039M to 1.905M ops/s, and p99 rises from 3.53 to 5.35 us.
They still reach 2.777 times the current original backend's throughput. These
results do not establish uniform gains or attainment of the earlier single-read
and scan parity targets. The asynchronous resident path and scheduling effects
need further attribution before assigning a cause to the disjoint-update change.

Separate instrumented runs show zero revision retries and zero byte-edit COW
for the final shared-update and hot-mix cases. Their baselines recorded 25,989 /
77,844 retries and 20,299,776 / 773,128,192 copied bytes respectively. The final
runs instead observe 86,254 / 250,065 contended resident latch attempts. This
supports avoiding snapshot/restart work; it does not imply contention vanished.
These counters are scheduling-dependent observations, not invariants.

## Disjoint-writer regression investigation

`changed-update-w4` assigns disjoint key ranges, not disjoint physical leaves.
Updates near a worker boundary can therefore contend on the same leaf. The
previous diagnostic run observed 75,591 contended resident acquisitions across
4,000,000 updates, with zero retries, COW bytes, and overflow reads. A separate
8,000,000-operation CPU profile attributed 18.49% of self samples to the three
semaphore acquire/release/bookkeeping symbols. These symbols also include other
locks; the share is not an exact leaf-wait cost. Sampling includes setup and
teardown. The previously reported 6.6% regression was a median difference from
one run whose throughput ranges overlap; it is not a constant per-operation tax.

The correction separates three cases:

- Uncontended inline mutations execute synchronous descent and byte editing.
  `ChangeAttempt` carries only a contended pin to the async continuation. The
  materialization/structural insert loop is kept in `insert_slow`.
- A validated level-one parent supplies a leaf hint. Writers avoid acquiring
  and releasing a shared leaf latch before requesting exclusive access. The
  exclusive path still validates page identity, type, fence and key position.
  Tree-level knowledge stays inside the index, not in BufferManager.
- Brief conflicts receive at most 32 additional nonblocking probes. Persistent
  contention enters the existing fair async queue. Probes cannot bypass queued
  writers and cannot spin indefinitely. Cancellation drops the waiter and pin;
  no guard spans acquisition of another guard or backing I/O.

`latch_probe_hits` counts contention resolved by these probes. Tests cover a
split published while a direct leaf writer waits, queued-writer fairness during
read probes, cancellation, and wakeup visibility. Exploration retained the
unsuccessful direct-latch-only candidate and the 8/32-probe candidates rather
than treating every intermediate change as a performance win.

Final evidence: three-version paired comparison (historical benchmark artifact; not tracked)
and full matrix with plots (historical benchmark artifact; not tracked).
Two reversed-order passes provide ten trials per version, each containing
2,000,000 updates (500,000 per worker), with diagnostics and profiling disabled.
The pre-optimization paged baseline reaches 2.033M ops/s, the previous version
1.924M, and the correction 2.217M: +15.2% over the previous version and +9.0%
over the earlier paged baseline. Median per-trial p99 is 3.73 / 5.47 / 3.53 us.
Throughput ranges overlap, so these are same-host observations, not fixed costs.

The shared-update control improves 64.4% and its p99 falls from 46.87 to 22.10 us;
single-worker changed updates improve 8.8%. The hot-mix control measures -1.9%
throughput and p99 16.99 -> 17.46 us. This small negative observation is retained,
not hidden by a claim of universal improvement.

In separate 4,000,000-update diagnostic runs, contended attempts fall from
76,822 to 27,631; 23,705 of the latter complete during probes, leaving 3,926 to
enter the async fallback. These counters count acquisition attempts, not actual
OS context switches. Recorded contention-resolution time falls from 643,105,153
to 69,197,824 ns, while retries and COW bytes remain zero in both versions.
This supports short-conflict handling as a contributor, not an exact attribution
of the earlier 6.6% to one cause. Probing trades CPU work for less queueing:
semaphore self-sample share rises from 18.49% to 22.84%, while whole-process
samples fall from 7,447 to 6,830. The profiles include setup and teardown.

Validation: default and `server/paged-index` workspace Clippy pass; all workspace
test targets compile. The paged-feature library suites pass 367 tests, including
62 index tests, and the release executable passes 42 paged-specific tests.
Code metrics remain clean. All 35 full-matrix cases complete with their operation
and final-state assertions. Diagnostic CSV parsing accepts retained 7/9-field
records and the current 10-field records.
