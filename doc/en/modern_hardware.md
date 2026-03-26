# Modern Hardware-Optimized Architecture

This document introduces a database runtime architecture optimized for modern hardware. The goal is not merely to rewrite a traditional synchronous server in async syntax, but to reorganize the data path around modern Linux kernel capabilities, multi-core CPUs, batched I/O, cache locality, and reduced lock contention.

In the MuduDB context, this architecture is mainly characterized by:

- `io_uring` as the primary I/O model
- asynchronous request lifecycles driven by state machines and continuations
- per-core workers and partition-local ownership to reduce cross-core contention
- lock-free or low-lock structures on the hot path
- batched submission, completion, wakeup, and response handling

This is an architecture overview document. It focuses on design intent, system structure, and performance rationale.

---

## 1. Why a Modern Hardware-Optimized Architecture

Traditional OLTP server designs often have these properties:

- network I/O and execution scheduling are split across multiple abstraction layers
- request lifecycles depend on thread switching, shared queues, and lock contention
- accept, read, parse, execute, and write are handled as loosely connected stages
- the same connection or session may move repeatedly across threads

Such models are functional, but on modern multi-core machines they tend to expose several bottlenecks:

- amplified lock contention
- higher cache miss rates
- worse tail latency
- too many syscall boundaries
- scheduler overhead inside the critical path

The core premise of a modern hardware-optimized architecture is:

1. Modern machines already provide substantial core-level parallelism. The main question is no longer just whether concurrency exists, but whether concurrency remains local.
2. Linux now provides lower-overhead async I/O facilities such as `io_uring`, making it possible to unify networking, files, timeouts, and cancellation under one completion model.
3. High-throughput hot paths should avoid “large shared state plus heavy locking” and instead rely on explicit ownership, local data, and minimal cross-core message passing.

---

## 2. Core Design Principles

This style of architecture usually follows these principles:

- `I/O first`: the I/O model is a top-level design constraint, not a later optimization pass
- `ownership first`: every connection, task, and state object should have clear ownership
- `locality first`: data, queues, and execution context should remain near the same core whenever possible
- `batch first`: submit, complete, wake, and flush operations should all be batch-friendly
- `async by runtime`: async complexity should be absorbed by the compiler and runtime, not pushed directly onto application authors

The typical target shape is:

- one bootstrap/control plane
- multiple worker threads
- each worker pinned to one core or partition
- each worker owning its own I/O ring, local queues, connection table, and executor

### 2.1 High-Level Architecture Diagram

```text
                          +-----------------------------------+
                          | bootstrap / control plane         |
                          | config / startup / shutdown       |
                          +----------------+------------------+
                                           |
                              server_mode = IOUring
                                           |
                     one worker thread per core / partition
                                           |
    --------------------------------------------------------------------------------
    |                          |                          |                          |
    v                          v                          v                          v
+-----------+            +-----------+            +-----------+              +-----------+
| Worker 0  |            | Worker 1  |            | Worker 2  |              | Worker N  |
| core-local|            | core-local|            | core-local|              | core-local|
+-----+-----+            +-----+-----+            +-----+-----+              +-----+-----+
      |                        |                        |                            |
      |  +------------------------------------------------------------------------+ |
      |  | worker-local runtime                                                   | |
      |  |                                                                        | |
      |  |  +--------------------+   +--------------------+   +----------------+  | |
      |  |  | io_uring ring      |   | protocol FSM       |   | procedure /    |  | |
      |  |  | SQ/CQ              |-->| decode / dispatch  |-->| component exec |  | |
      |  |  | accept/recv/send   |   | continuation park  |   | continuation   |  | |
      |  |  +---------+----------+   +----------+---------+   +--------+-------+  | |
      |  |            |                         |                          |        | |
      |  |  +---------v----------+   +----------v---------+   +------------v----+ | |
      |  |  | local ready queue  |   | partition-local conn   |   | KV syscall path | | |
      |  |  | lock-free / low-lock|  | table / ownership  |   | get / put /     | | |
      |  |  | hot-path scheduling|   | boundary           |   | range            | | |
      |  |  +---------+----------+   +----------+---------+   +-----------------+ | |
      |  |            |                         |                                  | |
      |  |  +---------v----------------------------------------------------------+ | |
      |  |  | cross-partition inbox / handoff queue                                  | | |
      |  |  | lock-free MPSC message passing when ownership must move            | | |
      |  |  +--------------------------------------------------------------------+ | |
      |  +------------------------------------------------------------------------+ |
      |                                                                             |
      +----------------------- optional partition handoff ------------------------------+
```

---

## 3. The Role of io_uring

`io_uring` is the foundation of this architecture. It is not just “a faster async socket API”. It turns I/O from “blocking syscalls plus sleeping threads” into a unified “submission queue plus completion queue plus user-space event loop” model.

### 3.1 The Two Core Queues

- `SQ`, the Submission Queue
  - user space describes and submits I/O requests to the kernel
- `CQ`, the Completion Queue
  - the kernel reports completed events back to user space

This allows a worker to model many operations through one event flow:

- `accept`
- `recv`
- `send`
- `read`
- `write`
- `fsync`
- `timeout`
- `cancel`

### 3.2 Why It Fits a Database 

A database  commonly needs to handle:

- high-frequency network traffic
- WAL or log writes
- data file I/O
- timeouts, cancellation, and backpressure

The key benefit of `io_uring` is that it can unify these operations under one completion-driven runtime model. The system can then be organized around “event completion” instead of “thread wakeup”.

### 3.3 Direct Performance Impact

With a good implementation, `io_uring` typically enables:

- fewer syscall transitions
- more natural batched submission and batched completion
- less blocking and fewer context switches
- a cleaner worker-local event loop
- a unified runtime for both network and file I/O

---

## 4. Per-Core Worker Architecture

A modern hardware-optimized runtime is usually not “a generic thread pool stealing arbitrary tasks”. It is more often “one ownership domain per core”.

### 4.1 Worker View

Each worker typically owns:

- one `io_uring`
- one partition-local connection table
- one local ready queue
- one cross-partition inbox
- one protocol state machine executor
- one procedure or component runtime context


### 4.2 Why Per-Core Ownership Matters

The main purpose of a per-core design is to minimize shared write paths.

If a connection stays on the same worker for most of its lifetime:

- its protocol state does not need to move across threads
- its receive and send buffers do not repeatedly invalidate caches across cores
- its task progression does not depend on shared locks
- its local queues and local memory remain cache-friendly

This model is more constrained than “any thread can process any connection”, but it is far more predictable under load.

---

## 5. Asynchronous Processing Model

A modern async architecture does not mean “write `async/await` everywhere and stop there”. What actually matters is that the request lifecycle must be decomposable, suspendable, resumable, and batch-drivable.

### 5.1 The Essence of Async

Async processing is not primarily about syntax. It means:

- a request does not block a worker while waiting on I/O
- execution can resume later from a continuation or state machine
- forward progress is driven by completion events

This is especially important for database procedures or component execution, because business logic is often naturally sequential:

1. read parameters
2. issue a query or KV operation
3. wait for the result
4. update state
5. return a response

Requiring developers to manually encode all of that as explicit low-level state machines would significantly raise complexity. A better model is:

- developers write logic close to sequential semantics
- the compiler and runtime convert blocking points into suspension points
- the worker resumes execution when the relevant `io_uring` completion arrives

### 5.2 Synchronous APIs for Users, Async Runtime Internally

An important design choice is that async interfaces should not be exposed directly to users.

From the user point of view:

- procedures are written against synchronous-looking interfaces
- control flow remains sequential and easier to reason about
- business logic is not forced to depend on futures, explicit wakeups, or manual async state handling

At deployment time, the transpiler lowers that synchronous procedure code into async procedure code suitable for the runtime. The execution model is:

1. the user writes synchronous procedure logic
2. the transpiler rewrites it into async control flow
3. the deployed procedure is executed by the runtime as continuation-driven async execution

This separation is deliberate:

- users keep a simple synchronous programming model
- the runtime still gets true async execution on top of `io_uring`

That is how the system keeps async performance characteristics without forcing every procedure author to write low-level async code directly.

### 5.3 Event-Driven Execution Chain

A typical execution chain looks like this:

```text
recv CQE
 -> decode request
 -> build execution task
 -> invoke procedure / syscall
 -> if pending: park continuation
 -> submit next I/O
 -> later CQE arrives
 -> wake continuation
 -> finish result encode
 -> send response
```

The advantage is that the worker main loop remains a “state progression” engine rather than a “blocking wait” engine.

The same flow can also be viewed from the programming model side:

```text
user writes synchronous procedure
 -> transpiler converts it into async procedure form
 -> runtime executes it as continuation-driven control flow
 -> io_uring completions resume execution at suspension points
 -> response is encoded and sent
```

---

## 6. Lock-Free and Low-Lock Design

“Lock-free” should not be interpreted as “there are literally no locks anywhere in the system”. It should be interpreted as:

- no mutexes on the hot path whenever possible
- cross-core communication primarily through one-way message queues
- minimal shared mutable state
- explicit ownership transfer instead of implicit shared access

### 6.1 Why Hot Paths Must Avoid Locks

In a high-concurrency database system, the real cost is rarely one individual lock operation. The real cost comes from:

- spinning or sleeping caused by lock contention
- cache-line bouncing across cores
- hot shared structures forcing latency to follow the slowest thread

That is why modern systems often prefer:

- worker-local connection tables
- worker-local ready queues
- connections that migrate only when necessary
- lock-free MPSC inboxes for remote handoff

### 6.2 Good Candidates for Lock-Free Structures

The best candidates for local lock-free design are usually:

- ready task queues
- continuation wake queues
- cross-partition message inboxes
- connection handoff queues

Structures that do not need aggressive lock elimination include:

- configuration management
- startup and shutdown control flow
- low-frequency metadata updates

In other words, engineering effort should focus on the actual hot path.

### 6.3 Lock-Free Design Depends on Ownership

Lock-free design without ownership constraints usually degenerates into harder-to-debug shared-state concurrency problems.

The correct order is:

- first define worker ownership
- then connection ownership
- then session ownership
- then task ownership
- only then optimize queues, slabs, arenas, and handoff mechanisms

---

## 7. Batching, Backpressure, and Pipelining

Another key property of a modern hardware-optimized architecture is batching.

### 7.1 Why Batching Matters

`io_uring` is particularly well-suited to these batched operations:

- batched SQE submission
- batched CQE harvesting
- batched continuation wakeup
- batched response writes

Batching reduces:

- syscall frequency
- queue access overhead
- scheduling churn

### 7.2 Backpressure Must Be Part of the Design

A high-performance system cannot focus only on “how to accept more work faster”. It must also define “when to defer, reject, or slow down work”.

Typical backpressure points include:

- ring depth approaching saturation
- a local ready queue growing too long
- accumulation of slow request classes
- send buffer buildup when write throughput falls behind

Backpressure should therefore be designed as a first-class runtime concern, not added later as an emergency fix.

---

## 8. Protocol Surface and Current SQL Status

For the current `io_uring` architecture, the runtime path should be understood as KV-first rather than SQL-first.

That means the current direction is centered around:

- lighter custom protocol handling
- partition-local execution
- KV-oriented operations such as `get`, `put`, and `range`


It is important to state the current status precisely:

- the `io_uring` runtime architecture exists
- the async continuation-driven execution model exists
- the KV-oriented path exists
- SQL support on the current `io_uring` path is not implemented yet

SQL support for this path is still under implementation.

So for now:

- SQL should be treated as an unfinished capability in the `io_uring` backend
- the current usable direction of the `io_uring` runtime is the KV-oriented path

---

## 9. Main Benefits of This Architecture

If fully implemented, this architecture should provide benefits in these areas:

- higher throughput
  - shorter I/O, scheduling, and execution paths
- lower tail latency
  - less lock contention and less scheduler noise
- better scalability
  - more natural partition-level scale-out as core count grows
- better cache locality
  - connections, state, and tasks remain worker-local whenever possible
- more predictable runtime behavior
  - ownership and handoff boundaries are explicit

---

## 10. Costs and Engineering Tradeoffs

This architecture is not free. It introduces real engineering complexity:

- state machine and continuation design becomes harder
- debugging is harder than in a simple blocking model
- error recovery paths must be extremely clear
- connection migration, cancellation, timeout, and shutdown paths become more subtle
- Linux `io_uring` support becomes an important platform dependency

A practical engineering strategy is usually:

1. define the runtime target and ownership model first
2. build the worker main loop and protocol progression second
3. gradually unify more I/O operations into the same completion model

---

## 11. What It Means for MuduDB

For MuduDB, the value of a modern hardware-optimized architecture is not just “higher speed”. It also allows the system to preserve three important properties at the same time:

- a natural synchronous programming experience for procedure authors
- a genuinely async, multi-core-aware runtime internally
- room to evolve toward component-based, KV-first, low-latency execution paths


If this design is reduced to one sentence, it is this:

> Use `io_uring` as the unified async completion model, per-core workers as the locality boundary, lock-free queues to reduce shared contention, and the runtime plus transpiler rather than user code to absorb async complexity.
