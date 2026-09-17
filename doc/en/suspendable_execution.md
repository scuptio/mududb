# Suspendable Procedure Execution: the Concurrency Model

MuduDB executes stored procedures as WebAssembly components. How a procedure
suspends when it reaches asynchronous I/O — and how many procedures may be in
flight on one OS thread — depends on which component world the package was
built against. This page describes the two models and their constraints.

## Sync world (legacy): thread-per-call, blocking

Packages built against the sync world (`wit/api.wit`) call **synchronous**
host functions. There is no suspension on this path: `call` runs each
invocation on a dedicated OS thread with its own current-thread runtime,
leases a pooled instance, invokes it to completion with `block_on`, and the
calling thread joins on the result.

Concurrency on the sync path therefore comes only from running invocations
on multiple OS threads — one blocked thread per in-flight call. Within one
thread there is no interleaving: a host call that reaches async I/O simply
blocks the thread until the I/O completes.

## Async world (current default): wasmtime-managed guest fibers

Packages built against the async world (`wit/async-api.wit`, all current
`.mpk` fixtures) import **async-lowered** host functions. The engine is
configured with `wasm_component_model_async` + `more_async_builtins`
(`enable_async`), and invocations go through `call_async`:

- wasmtime runs each `Store`'s component event loop. When the guest awaits an
  async host call and nothing else can progress, the `call_async` future
  returns `Poll::Pending` to the embedder and the suspended call state is
  detached from the thread — the thread is free to run other stores.
- **N component instances can be suspended mid-hostcall on one OS thread**,
  genuinely interleaved. The concurrency unit is the leased store: the number
  of procedures in flight per worker thread equals the number of pooled
  instances leased to them.
- The guest source stays **synchronous in any guest language** — no async/await
  in the procedure code. The suspension machinery lives entirely in the
  canonical-ABI glue generated for the async world; the native driver is an
  ordinary async task.
- In server mode this is already the execution shape: each connection task on
  the `WorkerTaskRegistry` drives its own `call_async` future, so concurrent
  connections suspend and resume independently on the io_uring/tokio workers.

## Host-side interleaving safety

Concurrent invocations share host-side machinery; the invariants that keep
them isolated:

- **worker_local rebinding.** Pooled instances are shared across workers.
  `LeasedInstance::set_worker_local` rebinds the store's WASI context to the
  caller's worker before *every* invoke, so a host call always sees the worker
  of the invocation it belongs to.
- **Transactions.** `invoke_procedure_async` begins a transaction per
  invocation (`pre_invoke`) on the invocation's own session context, and
  commits or rolls back exactly that context when the call returns — a
  failing invocation rolls back only its own transaction.
- **Instance pool exclusivity.** A lease moves the instance out of the pool:
  there is at most **one in-flight call per store**, enforced by ownership.
  An instance whose call traps is dropped instead of returned to the pool; a
  clean return (including a domain error encoded in the result bytes) makes
  the instance reusable.

## Requirements and limitations

- One in-flight call per `Store` (wasmtime rule; enforced by the pool lease).
- Host-context data must be `Send` (`call_async` futures may migrate across
  executor threads).
- Concurrent invocations on one app must use **distinct task sessions**: the
  SQL session registry and per-task connection are keyed by task id, so two
  invocations sharing a task id would alias the same session context.
- Async-world components require the async engine configuration (`enable_async`)
  and the `call_async` path; sync-world components use the thread-per-call
  `call` path.
- Pooled instances keep guest linear memory across invocations: procedures
  must be stateless functions of their parameters.

## Verification

The model is pinned by tests in `mudu_runtime`:

- `service/concurrent_wasm_suspension_test.rs` — two instances suspended
  mid-hostcall interleave on one OS thread (ordered host-call log), with a
  sequential control run producing identical results.
- `service/procedure_instance_pool.rs` tests — pool exclusivity, per-lease
  worker-local bindings, and reuse/rebinding of returned instances.
- `service/app_inst_impl_test.rs` tests — concurrent invocations commit
  independently; a failing invocation rolls back only its own transaction.
