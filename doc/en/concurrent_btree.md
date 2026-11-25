# Async paged B-link index

The optional `paged-index` feature replaces the relation's global-lock map
with `index::paged::PagedBTree`. It is opt-in; the default remains the baseline
index. The original phase-2 documents are design inputs, not descriptions of
the implemented synchronization or disk format.

## Enable and use

```sh
cargo build --manifest-path crates/db-kernel/Cargo.toml -p mudud --features paged-index
cargo test --manifest-path crates/db-kernel/Cargo.toml -p storage --features paged-index
cargo test --manifest-path crates/db-kernel/Cargo.toml -p index
```

The feature passes through mudud, mudu_runtime, server, worker and xcontract
to storage; the mudu_kernel facade also exposes it. A build without the
feature can open the same authoritative relation files. Cache files are
never the source of truth.

All tree operations that acquire latches are asynchronous and return `RS`.
`PagedBTree::new(context)` is a fallible, synchronous memory-only constructor.
`with_store` creates an empty tree on a fresh/private `PageStore`.
`with_row_ids` stores full u128 values inside leaf slots; generic values
instead occupy in-memory value cache buckets. Only the row-ID variant supports
`checkpoint` / `close_checkpoint` / `restore_row_ids`.

Applications embedding `Relation` call `close().await` after completing their
work. The Tokio and io_uring server shutdown paths do this after draining
request/replay work. A cancelled or failed close can be retried. `Drop` does
not flush. Writes after close fail. Reopening accepts a valid clean checkpoint
or rebuilds from the existing key/value streams and their WAL.

## Partition ownership

The tree and its buffer are neither Send nor Sync. Construct and use them on
one partition thread, normally a Tokio LocalSet. Resident operations use checked
local borrows; page access, pin counts and residency hints do not use atomic
read-modify-write operations. Conflicting local tasks suspend immediately;
spinning cannot advance another task on the same thread. Snapshot/revision and
page-generation checks remain necessary across asynchronous suspension.

With paged-index enabled, Relation construction requires the partition LocalSet.
Its sendable index handle routes foreign-thread requests to that owner. Local
ready operations are polled in place; Pending operations stay in a local task
and response cancellation aborts them before another poll. The shared metadata
service uses dedicated catalog owner executors. No tree or mutable page crosses
the message boundary. See the [current benchmark contract](paged_index_benchmark.md)
for single-thread task-count comparisons; older reports used a different runtime.

## Concurrency and representation

* Default encoded pages are 16 KiB, with a slotted directory, explicit little
  endian encoding, checksums and common-byte-prefix compression. Long keys
  use immutable overflow chains when their heads exceed the page-sized budget
  `(page_size - 192) / 4`. Explicit lexical mode retains a 512-byte cap to
  preserve fanout under cache pressure. Older overflow representations remain
  readable. Comparisons use the tuple comparator by default;
  encoded tuple byte order is not assumed to be SQL order. Explicit
  `with_lexicographic_keys()` enables unsigned-byte prefix/streaming comparison
  only when the caller guarantees that exact comparator contract.
* Internal nodes and leaves have exclusive high fences and right links.
  Ordinary leaf writers are independent. Splits use an async structural
  mutex, prepare new siblings before publication, validate old revisions,
  then publish under the affected page latches without an await.
* Page contents are immutable reference-counted byte images behind async latches.
  Resident inline-value lookups can finish under short read guards without
  snapshotting. Async fallback snapshotting shares bytes and releases the latch.
  Point descent and scans use an index-owned `NodeView` to access individual slots, without decoding
  all entries or allocating all keys. Decoded nodes remain operation-local for
  structural changes and reclamation, never a second resident cache.
  Each local image carries a consumer-owned validation-policy token.
  External/reloaded images are fully checked, including unvisited slots. Locally
  encoded/patched images are certified by checked builders; page ID and size are
  still checked on every access. There is no validation-cache lock or collision.
  Revision checks detect
  changes between asynchronous preparation and mutation; they do not authorize
  racing ordinary memory.
* `get` does not reconstruct the stored key; `get_key_value` does, preserving
  comparator-equivalent encodings. Inline values never acquire value-directory
  locks. Resident inline writes search and edit under the exclusive leaf latch;
  reacquiring that latch requires rechecking leaf type and fence. Generic
  replacement swaps only the directory value while holding its
  cache bucket lock and a revision-validated shared page latch; no page bytes change.
* Ordinary leaf inserts/deletes/replacements build prevalidated byte-edit programs.
  Packed inserts/deletes move record bytes and adjust slot offsets; prefix changes
  or capacity pressure fall back to recompression/splitting. Replacement changes
  only the fixed-width value and revision. After dropping the search snapshot,
  `prepare_edit` uses `Rc::make_mut`: unique images retain their byte allocation,
  while readers or an in-flight flush force COW. Preparation checks all ranges and
  performs COW before publication; dropping preparation leaves content unchanged.
  Publication cannot fail or suspend, invalidates/replaces the validation token,
  and advances the dirty epoch. Existing snapshots never change.
* Resident MBP1 images may have a stale checksum. The index-owned `PageWriteback`
  policy seals a private persistent image only during initialization/writeback;
  errors leave the page dirty. Checkpoint and eviction always persist a sealed
  image. Page magic, offsets and field widths are unchanged. The checksum is
  CRC32C over bytes `[0..40]` followed by `[48..]`, zero-extended into the
  existing little-endian eight-byte field. No baseline FNV fallback is supported.
* Split selection computes prefix/suffix and byte-count summaries once, then
  sizes each candidate in constant time. It preserves the old fit/balance/tie
  policy and constructs only the selected pair of nodes.
* There is no blocking mutex in the new index or DataRow path. The baseline
  backend is enclosed in an async mutex because its RefCell comparison context
  is not Sync. Relation and DataRow no longer assert unsafe Send/Sync.
* Mapping operations share an async scan gate. `range`, `clear`, extremes and
  `pop_*` take the exclusive side as needed. A range is a consistent mapping
  collection; mutations inside a returned DataRow remain governed by MVCC.
  Long scans delay mapping writers, but do not block the executor thread.
  Scans retain the first descent image/slot, locate the upper slot before
  materializing result keys, and load a sibling only when the range needs it.
* Multi-page publication uses ordered `try_write` acquisition; on contention,
  acquired latches are released before asynchronous waiting. No page latch
  crosses backing I/O or a second page-latch wait.

## Buffer and row cache

The default tree has 256 frames and four cold-path directory partitions.
A bounded manager-owned table caches frame-index/generation hints by page ID.
Pin acquisition and eviction claims share a thread-local Cell state word; identity is
checked after pinning. A stale hint falls back to the partition directory.
`try_pin` also tries that directory without waiting, so references decoded from
bytes still have a synchronous resident fast path. Hints use no raw pointers
and are not serialized into index pages.

Clock sampling cools pages. A cooling hit makes a page hot again; normal hot
accesses do not update a global LRU. Unpinned dirty victims are written before
reuse. Same-partition cold operations serialize asynchronously and coalesce
loads. Exhausted frame reservations return a storage error rather than wait
while holding pins indefinitely. Flushes issue up to one concurrent I/O per
partition. Scans prefetch at most one right sibling. Hints and prefetch can be
disabled independently with `BufferOptions` for ablation tests.

`OrderedFile` retains submitted write futures across caller cancellation.
Before a retry/read/flush, the retained write is driven to completion. This
prevents an old cancelled kernel write from completing after a newer retry.
Cache-miss cancellation releases its frame claim and leaves the old resident
image and directory intact. Failed writeback does not make a dirty page
reusable.

Relations use `key -> u128 RowId`, not a resident directory of generic values.
The row cache has 16 cache buckets with a default 4 MiB version-byte budget per cache bucket.
DataRow updates adjust shared byte counters; hot lookups do not rescan every
row. External DataRow clones pin shared identity. Only an unshared row may be
evicted, after its complete version image has been written. Row directories
use checked offsets for the existing u64 tuple allocator; identifiers are
never silently truncated.

Budgets exclude active pins, returned result collections, allocator overhead
and transient encoding buffers. A large active row or result may exceed a
cache budget. Decoded page snapshots also have bounded per-operation costs.
These are cache limits, not a hard process RSS guarantee.

## Cache files and recovery

Each key-file basename has four rebuildable companions:

* `.index-pages`: fixed-offset encoded B-link and overflow pages;
* `.index-rows`: append-only complete version images;
* `.index-row-directory`: fixed-offset row image extents/checksums;
* `.index-manifest`: clean marker, checksum and checkpoint state.

The manifest includes root/allocation state, next tuple ID, schema/table/
partition identities and recovered key/value WAL extents. It is consumed and
invalidated durably before the cache can mutate. Close excludes writes,
flushes authoritative WAL/data, flushes index/row images, persists the
manifest body, and finally persists its clean marker. Cancellation or errors
before that final publication cannot validate a partial checkpoint.

An incomplete relation write permanently taints that open cache instance.
This is necessary because deferred updates can publish row versions before
their file append. Such an instance cannot publish a clean checkpoint; reopen
rebuilds it. Missing cache files, an invalid manifest, a changed authority
stamp, or a corrupt root cause rebuild. Corruption discovered later in a cold
page/row is reported as an error; the already-consumed marker ensures a later
reopen rebuilds. There is no index WAL or crash-time incremental recovery.

Fallback rebuild truncates cache files. Online maintenance also reuses
unreachable index node and overflow page slots; it does not shrink the file's
high-water size or compact the append-only row-version file.

## Buffer service boundary

The rank-0 `page_buffer` crate contains the non-generic, object-safe
`BufferManager` service and the default `ClockBufferManager`. B-tree holds
`Rc<dyn BufferManager>`; no associated page type, `Node`, tuple comparator,
codec, or manifest exists in the buffer crate. Existing public buffer/store
names remain re-exported from `index::paged` for compatibility.

`PageRef` identifies a page within one manager. `PagePin`, `PageImage`, and
read/write guards expose bytes rather than frames. Pins and reservations are
lifetime-bound to a `BufferAccess` lease (lifetimes are not generic page types).
A shared lease covers an entire traversal, including copied IDs; an exclusive
lease drains prior traversals for reclamation. Backend implementations can use
`BufferDomain` and `ResidentPage` to construct safe handles without exposing
Frame internals to their callers. Cross-manager leases, references and
reservations are rejected.

Allocation first reserves a page number, allowing the index to encode its
MBP1 identity, then initializes a complete persistent image through its writeback
policy. Active
reservations cannot be swept; abandoned ones are reclaimable. The index prepares
all replacement images before publication, acquires page guards in order,
validates revisions, and calls `prepare` on every guard. Only then does it
publish the prepared replacements, with no await, failure or latch release
between them. Publication automatically advances dirty versions. Buffer flush
persists pages; relation checkpoint publishes the consistent external manifest.

Inject a fresh buffer as follows (all page spaces remain per-tree):

```rust,ignore
let buffer: Rc<dyn BufferManager> = Rc::new(ClockBufferManager::new(
    options, store, AllocatorState { next_page: 1 }, None,
)?);
let tree = PagedBTree::with_row_ids_buffer(context, buffer).await?;
```

Use `with_buffer` for generic in-memory values and `restore_with_buffer` for
row-ID checkpoint recovery. The injected manager's page space belongs
exclusively to that tree. Existing synchronous constructors still encode and
seed the initial root in memory; they do not perform I/O. MBP1 pages and existing
manifest fields remain byte-compatible. Reopen supplies allocator state to the
manager constructor; tree code never modifies allocator state directly.

The byte-page boundary keeps format knowledge inside the index. `PageEdit` is a
format-independent checked byte program; `ImageValidator` is a static policy
identity; `PageWriteback` serializes resident bytes. None mentions B-tree nodes,
MBP1 offsets, checksums, or tuple ordering. Alternative managers must use
`PageImage::writeback_bytes()` before writing to their stores. The returned bytes
are borrowed for ordinary raw images, owned when a consumer policy is present.
Optional `BufferManager::try_access()` defaults to `None`, retaining compatibility
with alternate asynchronous backends. The clock manager's fast lease path,
page read/write latch paths, and tree read gate use nonblocking attempts with
asynchronous fallback; queued exclusive maintenance retains lock fairness.

`NodeView` avoids full-node decoding on ordinary reads. Short compressed keys use
stack scratch space with the default tuple comparator. With the explicit lexical
capability, comparisons visit prefix/suffix slices directly and can reject an
overflow key from its resident head; ambiguous comparisons stream validated
chunks without allocating a complete key. Returning owned keys still materializes
them. The capability is not persisted and must be re-enabled after restore.

The [performance matrix](paged_index_benchmark.md) retains the original, P0/P1/P2,
and subsequent five-optimization measurements. There is no raw-pointer swizzling,
racing read of mutable bytes, new disk format, or generic BufferManager.

## Online maintenance

`PagedBTree::merge_pages()` merges fitting adjacent siblings at leaf and
internal levels while ordinary point operations continue. It uses the same
async structural mutex as splits. The parent, surviving left page and retired
right page are pinned, latched with ordered try-write acquisition, and revision
validated. Publication performs no I/O or await. The retired right page becomes
a forwarding node to the survivor, so a reader with an earlier path can finish
safely; a writer with an earlier leaf snapshot must retry. Empty roots contract.
A pass may skip a busy pair; later passes retry it, and repeated passes can
compact levels made sparse by earlier lower-level merges.

`PagedBTree::reclaim_pages()` takes the exclusive async operation gate as a
grace period. After prior traversals, page pins and overflow snapshots have
finished, it walks reachable child, sibling and overflow links, invalidates
unreachable frames and makes their page IDs available to the allocator. It
also recovers orphan allocations from cancelled split/key preparations. No
page ID is reused while a previous operation can still hold it. A failed or
cancelled sweep can defer reuse but cannot free a live page. The sweep pauses
index operations, performs async I/O, and uses metadata proportional to the
page high-water mark. It is not an incremental constant-memory collector.
Free lists are reconstructed by a sweep after reopen rather than persisted.

`PagedBTree::maintain()` runs both phases and returns merge/reuse counts.
`Relation::maintain_index()` exposes this for the paged relation backend.
Maintenance is explicit, not automatically scheduled on each delete. MVCC
`tombstone` versions remain until a separate visibility-aware vacuum decides
they can be removed; merging does not discard historical rows.

## Online checkpoint

`Relation::checkpoint().await` checkpoints without closing the relation.
`WorkerStorage::checkpoint_relation_indexes().await` applies it to hosted
relations individually; it is not a transactionally atomic database backup.
The relation's async lifecycle barrier drains existing writes and delays new
ones throughout the flush. Readers continue. Both ordinary writes and deferred
delta writes participate in this barrier. The tree's corresponding
`checkpoint()` API also remains open, but direct users must implement the
external manifest invalidation protocol themselves.

The first subsequent relation mutation durably invalidates the clean marker
before changing mappings, row versions, or authoritative streams. Maintenance
also invalidates it. Publication leaves the in-memory state requiring
invalidation even if cancellation occurs after the clean-byte write has been
submitted. Retained ordered writes are drained before the invalidation write,
so a delayed clean byte cannot resurrect a stale checkpoint. Row eviction
recognizes unchanged persisted snapshots and leaves their durable directory
extents intact after publication. Failed row-image writes forget their cached
digest before I/O, so a retry cannot skip necessary persistence.

This is a quiescent, in-place online checkpoint: the database stays open, but
writers do not run through checkpoint I/O. It is not a copy-on-write snapshot
or a fuzzy checkpoint with incremental index-WAL replay. After subsequent
writes invalidate it, crash recovery rebuilds from the authoritative streams
and their WAL. A later online checkpoint or clean close makes the cache
restartable again.

## Validation and benchmarks

The expanded [performance matrix](paged_index_benchmark.md) compares the paged
index and original in-memory index across 31 cases, with repeated trials,
latency distributions, cache counters, raw CSV, and reproducible reports.
The older ablation benchmark below is retained for historical comparisons;
its periodic latency samples select writes in the 90/10 workload and should
not be interpreted as the latency distribution of all operations.

Unit tests cover random baseline parity, boundaries, variable/overflow keys,
concurrent split/update/scan, serial histories for competing replacements,
frame reuse, cancellation, write errors, full-width RowId checkpoints and
row-cache eviction with live shared identities. Maintenance tests cover
internal/root contraction, stale forwarding paths, grace-period waiting,
overflow/page-ID reuse and concurrent split/merge/write histories. Storage
tests inject errors throughout manifest publication, cancel a submitted clean
write, retry failed invalidation, and check unchanged row extents after eviction.
Storage tests also cover clean
restart, consumed markers, manifest corruption and failed-write taint.
Buffer contract tests use arbitrary non-index byte images, an independent
single-page backend, reservation cancellation, wrong-manager rejection and
writeback races. A frozen MBP1 fixture checks persisted-format compatibility.
A compile-fail doctest prevents pins from escaping their access lease. The
buffer crate tests local FIFO waiting, cancellation and wakeup; compile-fail
tests reject Send/Sync trees and Send buffers. These checks are not a proof of
the whole database or of asynchronous filesystem code.

```sh
cargo test --release --manifest-path crates/db-kernel/Cargo.toml -p index \
  paged::bench::index_local_tasks_and_hint_ablation -- --ignored --nocapture
cargo +nightly miri test --manifest-path crates/db-kernel/Cargo.toml -p index \
  paged::tests::codec_rejects_corruption
```

The benchmark prints throughput, sampled p99, scan throughput and cache
counters for 1/2/4 workers, with hints enabled and disabled. It uses an
in-memory backing store, 4096 keys and a 90% read / 10% replacement mix; it is
not a durable OLTP or out-of-memory benchmark. Before the byte-page refactor, on one development run, hints
improved the four-worker result from about 0.38M to 1.37M ops/s. The baseline
single-thread baseline was about 2.36M ops/s versus 0.75M for the new tree's
single-thread configuration. The feature therefore remains opt-in; these
numbers do not establish a general performance win.

After the byte-page refactor, one local release run produced the following
results. The host was not isolated from other build work, so these are
regression observations rather than a controlled hardware comparison:

| Workers | Hints off (ops/s) | Hints on (ops/s) | Hints on sampled p99 (µs) |
| --- | ---: | ---: | ---: |
| 1 | 41,968 | 38,505 | 130.8 |
| 2 | 67,368 | 67,170 | 176.8 |
| 4 | 99,515 | 118,320 | 476.7 |

These results are substantially below the previous decoded-node-cache run.
Those historical measurements precede the node-view and five-optimization work.
The current implementation and comparative results are described above and in
the linked performance matrix; the feature remains opt-in.

The implementation borrows the hot-reference, cooling, compression and I/O
ideas from [LeanStore (ICDE 2018)](https://15721.courses.cs.cmu.edu/spring2020/papers/23-largethanmemory/leis-icde2018.pdf)
and its [subsequent implementations](https://github.com/leanstore/leanstore).
It does not implement their unsafe optimistic page reads or compact raw-pointer
swips. Merging, reclamation and checkpoints use the async protocols described
above rather than reproducing LeanStore's synchronization and recovery protocols.


## Related checksum work without format changes

The shared `mudu::common::crc::crc32` helper uses hardware-dispatched CRC32C
with portable fallback. Its output matches the previous CRC-32/ISCSI
implementation, so ordinary data-page checksums remain byte-compatible.
`crc64` and `calc_crc` retain CRC-64/XZ semantics.

Row-cache persistence reuses the checksum computed for its immutable exported
snapshot instead of hashing the same bytes twice. The 64-bit FNV digest still
serves both integrity checks and the unchanged-row decision; this optimization
does not narrow that digest or change checkpoint publication ordering.

Replication segment hashing streams the domain prefix, previous segment hash,
and each length-framed payload into incremental CRC-64/XZ states. It preserves
all four checksum lanes and their exact byte order, while removing the segment
concatenation and four per-domain input allocations. Tests compare the new
chain against the old concatenation algorithm, including empty frames, boundary
changes, large frames and nonzero predecessor hashes. This does not redefine
the chain as a cryptographic hash.
