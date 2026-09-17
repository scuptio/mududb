# Agent Guide for `mududb`

This file contains conventions and checklists for AI agents working on the `mududb` workspace.

## Repository Layout

The repository root is **not** a cargo workspace. Crates are grouped under
`crates/` into four independent cargo workspaces, each with its own
`Cargo.toml` / `Cargo.lock`:

- `crates/common` — foundation (`mudu`), sys abstraction (`mudu_sys*`), types,
  bindings, contract, SQL parser, client
- `crates/db-kernel` — `mudu_kernel`, `mudu_runtime`, `mudud`, `mudu_adapter`,
  `testing`
- `crates/sdk` — `mududb` SDK, `example/*`, `bindings/*`, `mudu_api*`
- `crates/tools` — `mudu_cli`, `mudu_gen`, `mudu_transpiler`, `mpm_build`,
  `mpm_crate`, `mpm_install`

Cross-group path dependencies use the `../../<group>/<crate>` form. All four
workspaces share the repository-root `target/` directory (root
`.cargo/config.toml` sets `[build] target-dir = "target"`). Run cargo commands
per group workspace, e.g. `cd crates/db-kernel && cargo test --workspace`.

Whole-project orchestration lives in the root `Makefile.toml` (cargo-make):

- `cargo make` / `cargo make build` — release build of all four group
  workspaces plus the standalone `crates/sdk/mudu_api/rust` workspace, then
  `install-tools` (mgen/mtp/mpm-build/mpm-crate/mpm-install/mudud/mcli into
  `~/.cargo/bin`) and the `wallet.mpk` example package
- `cargo make check` — fmt + clippy + test compilation across the four groups
  (delegates to `script/shell/check_all.sh`)
- `cargo make test` — `cargo test --workspace` for every workspace
- `cargo make test-e2e` — end-to-end suite via `crates/db-kernel/testing`
  (builds wallet / wallet-as / wallet-cs / wallet-py / wallet-go / wallet-c
  mpk packages first; needs Node, the .NET 10 SDK, componentize-py, TinyGo
  and wasi-sdk)
- `cargo make package-wallet-as` / `package-wallet-cs` / `package-examples` —
  optional guest packages requiring the external toolchains

Shared-target invariant: crates consumed as path dependencies across group
workspaces must stay **rlib-only** (no `cdylib` in `[lib] crate-type`). A
cdylib crate-type makes cargo emit final artifacts under unhashed names
(`lib<name>.{rlib,so}`) in the shared `target/`; each group workspace then
overwrites the same file and downstream compiles fail with E0463 ("can't
find crate") in a self-perpetuating ping-pong. `sys_interface` and
`sys_interface_standalone` hit exactly this; both are rlib-only now. If the
`uniffi-bindings` workflow ever needs the cdylib back, isolate that build's
target dir instead of re-adding cdylib to the shared one.

Known issue: `doc/**/*.md` still contains crate links of the form
`../../../<crate>` that predate the `crates/<group>/` layout and are stale;
fixing them is tracked as a separate batch task.

## Before You Change Code

1. Read `ARCHITECTURE.md` for crate layering and dependency rules.
2. Run `cargo fmt` in each group workspace you touch.
3. Run `cargo clippy --workspace --all-targets -- -D warnings` per group before and after changes.
4. Run `cargo test --no-run --workspace` per group to ensure all test targets compile.
5. Or simply run `bash script/shell/check_all.sh`, which executes steps 2–4
   (in check mode) sequentially across all four group workspaces.
6. When you change the mgen entity template or SQL in example procedure
   sources, run `cargo make check-sql` in the affected examples to validate
   SQL literals against `sql/ddl.sql` (see `doc/dev/sql_subset.md`).

## Memory-Constrained Environments

`.cargo/config.toml` caps build/test memory so `cargo test` survives an ~8 GB
budget: `jobs = 4`, a rust-lld linker wrapper (`.cargo/cc-lld.sh`), and
`RUST_TEST_THREADS = 4`. This config applies to every group workspace through
directory inheritance, and its `target-dir = "target"` points all builds at
the shared repository-root `target/`. Each group workspace root `Cargo.toml`
also sets `profile.dev.debug = 1` (line tables only) to shrink linker memory.
Do not raise these limits without re-measuring peak memory.

## Formatting

- The entire workspace is formatted with `cargo fmt` using the default Rust style.
- The pre-commit hook (`.githooks/pre-commit` / `.githooks/pre-commit.ps1`) runs `cargo fmt -- --check` per group workspace first; commits are blocked if formatting is off.
- CI also enforces formatting with a dedicated `cargo-fmt` job that loops over the four groups.

## Lint Rules

The workspace treats warnings as errors in CI. Do not introduce new warnings.

### Disallowed APIs

`clippy.toml` forbids many `std` / `tokio` APIs outside of `mudu_sys_impl`. Use these replacements:

| Disallowed API | Use Instead |
|---|---|
| `std::fs::*` | `mudu_sys::fs::sync::*` (sync) or `mudu_sys::fs::async_::*` (async) |
| `std::env::var` / `temp_dir` / ... | `mudu_sys::env_var::*` |
| `std::sync::Mutex` / `RwLock` | `mudu_sys::sync::SMutex` / `SRwLock` |
| `std::thread::spawn` / `sleep` | `mudu_sys::task::sync::spawn_thread` / `sleep_blocking` |
| `std::net::TcpListener` / `TcpStream` | `mudu_sys::net::sync::StdTcpListener` / `SStdTcpStream` |
| `std::time::Instant::now` / `SystemTime::now` | `mudu_sys::time::instant_now` / `system_time_now` |
| `tokio::time::timeout` / `sleep` | `mudu_sys::timeout` / `mudu_sys::sleep` |
| `std::process::Command` | `mudu_sys::process::Command` |

Do **not** add module-level `#![allow(clippy::disallowed_methods)]` or `#![allow(clippy::disallowed_types)]` to work around these rules. If a file legitimately cannot use `mudu_sys` (e.g. inside `mudu`), place the minimal scoped `#[allow(...)]` on a wrapper function and document why.

### Crate Purity

- **`mudu`** must stay pure: no I/O, no environment variables, no direct `std::fs` / `std::net` / `std::thread` usage except compile-time constants like `std::env::consts::ARCH`.
- If a utility needs I/O, put it in `mudu_utils` and use `mudu_sys` wrappers.

## Build Scripts

- Use `mudu_build_common` for shared logic (path resolution, copy-if-changed, workspace version parsing, ts-const generation).
- Add `mudu_build_common` to `[build-dependencies]` when needed.
- New or regenerated files must include a header comment such as `// Generated by <crate>. Do not edit manually.`

## Tests

- Reuse helpers from `testing::support` instead of duplicating them in each integration test file.
- Keep `TestContext` / `RunningServer` local to a test file if they have file-specific behavior.
- All integration tests must still pass `cargo clippy -p testing --all-targets -- -D warnings`.

## CI / Tooling

The following checks run in CI (see `.github/workflows/ci-hardening.yaml`);
each runs once per group workspace (`crates/common`, `crates/db-kernel`,
`crates/sdk`, `crates/tools`) because every group has its own `Cargo.lock`:

- `cargo fmt -- --check`
- `cargo deny check --config <repo-root>/deny.toml bans licenses advisories sources`
- `cargo udeps --workspace --all-targets`
- `cargo hack check --workspace --each-feature --no-dev-deps` (non-blocking: pre-existing feature-interaction issues in external crates)
- `cargo outdated --workspace --root-deps-only` (non-blocking)

When adding a new crate, always include a `license = "..."` field and prefer workspace dependencies.

## Language Bindings (AS / C# / Python / C / Go / Rust)

The bindings share one source of truth and one naming scheme; keep them that
way (full details: `doc/dev/binding_api_surface.md`).

- **Source of truth**: `crates/common/mudu_binding/wit/` (23 × `uni-*.wit`).
  Type or wire-format changes happen only there; all six languages'
  codecs are regenerated with mgen in the same commit. Generated files
  ("Generated by mudu_gen. Do not edit manually.") are never hand-edited —
  change the mgen templates/backends instead.
- **Canonical module paths**: every binding is rooted at `mududb` with shared
  segment names — `mududb.types` (generated Uni* types) and `mududb.codec`
  (MessagePack runtime + MSSP frame codec) exist in all six languages;
  `sql` / `db` / `fs` / `result` / `sys` / `mock` exist where the language
  carries that capability. npm and the Go module are exceptions in form
  only: the org scope makes the npm root `@mududb/mududb`, and the Go
  module is `github.com/ybbh/mududb_p/bindings/go` with packages
  `types`/`codec`; the C binding carries the canonical names literally as
  include paths (`mududb/types/Uni*.h`, `mududb/codec/mpack.h`) — identical
  subpaths.
- **Hand-written layer stays thin**: only facades (db/fs/sys entry points,
  mpack runtime, C# mock backend) plus the per-language record bridges for
  user-defined procedure types (`--type-wit`; AS `assembly/record.ts`, C#
  `mudu_sys/RecordBridge.cs`, py `codec/bridge.py`, C
  `codec/record_bridge.{h,c}`, Go `types/bridge.go` — authoring guide:
  `doc/dev/custom_types.md`) are hand-written, following the canonical
  surface table in `doc/dev/binding_api_surface.md`. Update that document
  first when adding a facade capability. The facade exists in all four
  guest languages: AS (reference), C#, Rust (`mududb::db/fs/result`), and
  Python (`mududb.db/sql/result/fs/sys`, running in componentize-py
  guests — pipeline and examples: `crates/sdk/example/wallet-py`,
  `crates/sdk/example/py-spike`; mtp front-end: `mudu_transpiler`'s
  `python` subcommand).
- **Consistency gates**: `bash script/ci/check_bindings_regen.sh` (regen
  must be byte-clean) plus each language's golden-corpus tests
  (`crates/db-kernel/testing/fixtures/golden/v1/`, produced by the Rust host
  codec, authoritative). CI: `.github/workflows/bindings.yaml`.
- **Publishing**: npm `@mududb/mududb`, NuGet `mududb`, PyPI `mududb` —
  independent semver, released from `as-v*` / `cs-v*` / `py-v*` tags.
  Packages ship pure language artifacts only (no Rust sources). The C and
  Go bindings currently ship as in-repo source only (no registry/tag
  release).

## Deterministic Simulation (internal)

- `mudu_sys` declares an empty `ds` feature, and several crates pass it through
  (`mudu_kernel/ds`, `mudu_runtime/ds`, `testing/ds`). In this repository
  enabling `ds` is a **no-op**: it exists so the `#[cfg(feature = "ds")]`
  gates in code and tests keep a valid feature reference.
- The actual deterministic simulation backend is closed source and maintained
  in a separate internal repository, which wires itself in via a manifest
  overlay. Do not add dependencies on it here, and do not "fix" the empty
  `ds` feature — it is intentionally inert.
- The overlay is applied to an internal scratch copy only; this checkout is
  never modified by internal test runs. Changing any group workspace
  `Cargo.toml`, `crates/common/mudu_sys/Cargo.toml`, or
  `crates/common/mudu_sys/src/lib.rs` will trip the internal
  overlay's baseline hash check and fail those runs with a drift error until
  the internal patch set is regenerated — this is expected.

## Documentation

- Add doc comments to new public items.
- For core crates, enable `#![warn(missing_docs)]` only after the crate is fully documented to avoid `-D warnings` failures.
