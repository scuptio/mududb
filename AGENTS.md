# Agent Guide for `mududb`

This file contains conventions and checklists for AI agents working on the `mududb` workspace.

## Repository Layout

The repository root is **not** a cargo workspace. Crates are grouped under
`crates/` into independent cargo workspaces, each with its own
`Cargo.toml` / `Cargo.lock`:

- `crates/common` — foundation (`mudu`), sys abstraction (`mudu_sys*`), types,
  bindings, contract, SQL parser, client, CAF container codec (`mudu_caf`)
- `crates/db-kernel` — the kernel is split into `mudu_kernel` (thin facade) +
  sixteen kernel crates (`catalog`, `index`, `page_buffer`, `wal`,
  `xengine`, `storage`, `meta`, `command`,
  `sql`, `conn`, `xcontract`, `worker`,
  `repl`, `backup`, `server`, `ws_gateway`
  (WebSocket gateway)), plus `mudu_runtime`
  (pgwire/HTTP serving backend), `runtime_wasm` (WASM procedure host),
  `runtime_libsql` (libsql connector),
  `mudud`, `mudu_adapter`, `mudu_raft` (Raft consensus, a vendored port of
  scuptio/scupt-raft), `sys_interface_standalone`, `testing`,
  `catalog-testing` (catalog test fixtures)
- `crates/sdk` — `mududb` SDK, `bindings/*`, `mudu_api*`
- `example/mpk` — separate consumer workspace for the guest-SDK examples
  (`app1`, `wallet`, `vote`, `key-value`, `ycsb`, `tpcc`)
- `crates/tools` — `mudu_cli`, `mudu_gen`, `mudu_transpiler`, `mpm_build`,
  `mpm_crate`, `mpm_install`
- `crates/bindings` — standalone cdylib workspace `mudu_adapter_ffi` (native
  adapter ABI), isolated from the shared `target/`
- `verification/` — Aeneas + Lean formal verification (design:
  `doc/en/formal_verification.md`; operations: `verification/README.md`).
  `extract/` is the Cargo extraction entry (zero-dependency standalone
  workspace), `lean/` the pinned Lean project (per-module
  `generated.lean`/`spec.lean`/`proof.lean` mirroring the production paths),
  `targets.toml` the registry of verified targets (extraction config,
  theorems, allowed axioms, self-test mutation), `toolchain.lock.toml` the
  SHA256-pinned tool bundles. `script/verification/` holds the install /
  check / reporting scripts; `cargo make verify*` delegates to them
- `script/` — build/CI/bench automation; `script/ai_tool/` holds AI-assisted
  development prompts and helpers (not part of any build)

Cross-group path dependencies are declared in each group workspace's
`[workspace.dependencies]` block using the `../<group>/<crate>` form (member
crates consume them via `workspace = true`); a few member-level direct
dependencies use the `../../<group>/<crate>` form. All
workspaces share the repository-root `target/` directory (root
`.cargo/config.toml` sets `[build] target-dir = "target"`). Run cargo commands
per workspace, e.g. `cd crates/db-kernel && cargo test --workspace`.

Whole-project orchestration lives in the root `Makefile.toml` (cargo-make);
run `cargo make help` for the grouped task list (core built-in flows are
skipped via `skip_core_tasks`, so `cargo make --list-all-steps` shows only
this project's tasks):

- `cargo make` / `cargo make build` — release build of the four `crates/`
  group workspaces plus the standalone `crates/sdk/mudu_api/rust` workspace
  (compile only); `cargo make dist` additionally runs `install-tools`
  (mgen/mtp/mpm-build/mpm-crate/mpm-install/mpm-tool/mudud/mcli into `~/.cargo/bin`)
  and packages the `wallet.mpk` example
- `cargo make install` — install the workspace binaries via
  `script/build/install_binaries.py` with argument pass-through
  (`--debug`, `--root <dir>`, `--dry-run`, `--with-adapter-ffi`); builds via
  fingerprint-incremental `cargo build --bins` and copies the artifacts
  (byte-identical installs are skipped), then prints a post-install summary
  (path, version, size) of every artifact. `install-tools` is the fixed
  release-only form used as a `dist`/package dependency
- `cargo make package-release` — server release tarball into
  `target/release-artifacts/<version>/` via `script/release/package_release.py`;
  the Release workflow (`.github/workflows/build-release.yaml`) runs the same
  script, so local and CI packaging cannot drift
- `cargo make package-sdk-registry` — pack the npm/NuGet/PyPI registry packages
  into `target/sdk-dist/registry/` (per-file sha256 + manifest; no credentials)
  via `script/release/pack_registry.py`. Publishing is a separate command that
  only verifies and pushes (dry-run by default, `--apply` to upload):
  `python3 script/release/publish.py npm,nuget,pypi,github-sdk,github-runtime`
- `cargo make check` — fmt + clippy + test compilation across all workspaces
  plus the repo invariants (delegates to `script/ci/check_all.py`); split into
  single steps via `check-fmt` / `check-clippy` / `check-test-compile` /
  `check-deny` / `check-deps` / `check-arch` / `check-test-feature` /
  `check-metrics` / `check-todo-panic`, or combine arbitrarily with
  `cargo make check -- --only fmt,clippy`
- `cargo make check-metrics` — only the code-metrics steps of `check`
  (`mudu_metrics` over `crates/` and `example/`), for quick iteration against
  the `code_metrics.toml` line limits
- `cargo make check-todo-panic` — advisory per-crate TODO / panic-surface
  report over hand-written production code (`script/ci/check_todo_panic.py`;
  excludes generated files and all test code, never fails)
- `cargo make test` — `cargo test --workspace` for every workspace
  (delegates to `script/ci/run_all_tests.py`; the `test-group-*` tasks run a
  single group)
- `cargo make test-e2e` — end-to-end suite via `crates/db-kernel/testing`
  (builds wallet / wallet-as / wallet-cs / wallet-py / wallet-go / wallet-c
  mpk packages first; needs Node, the .NET 10 SDK, componentize-py, TinyGo
  and wasi-sdk)
- `cargo make coverage` — one merged llvm-cov report (unit tests of all
  group workspaces + the testing crate's e2e tests) via
  `script/coverage/run_coverage.py`; prints overall/per-group totals and the
  HTML report link
- `cargo make bench-smoke` — performance regression gates
  (`script/bench/bench_gate.py`): the paged-index tasks suite at smoke scale
  plus a one-warehouse stored-procedure TPC-C run against a locally managed
  mudud, each compared with its committed baseline in
  `script/bench/baselines/`. Gates are machine-independent counters (exact)
  and abort-rate/floor thresholds; wall-clock throughput and latency are
  recorded for trend but never gated. After an intended change to
  benchmarked code, re-record with `cargo make bench-baseline-update`
  (`-paged` / `-tpcc` for a single one) and review the baseline diff in the
  same commit — same convention as binding regen. `bench-smoke-tpcc` builds
  `tpcc.mpk` first (`package-tpcc`, needs the wasm toolchain).
- `cargo make udeps` / `outdated` / `dep-audit` — dependency hygiene
  (unused deps via cargo-udeps on the pinned nightly, stale direct deps via
  cargo-outdated), run per group workspace by `script/ci/dep_audit.py`; each
  accepts `--group <id>`. Independent workspaces continue after failure;
  `udeps` fails the final result and `outdated` remains advisory. Both save
  complete logs and summaries under `logs/dependency-audit/`. cargo-deny drift lives in `check`
  as `check-deny` (deterministic bans/licenses/sources locally; the
  time-varying advisories check runs only in CI, `ci-hardening.yaml`)
- `cargo make package-wallet-as` / `package-wallet-cs` / `package-examples` —
  optional guest packages requiring the external toolchains
- `cargo make doctor` — probe the toolchain and report what each tool
  unlocks (which example packages, e2e, coverage, or release steps become
  available); missing entries show a per-OS shell install command
  (Linux/macOS/Windows; preview another platform via `DOCTOR_OS=macos`), a
  descriptive note, and a hyperlink to the tool's install documentation; the
  header names the shell the commands are written for ($SHELL on
  Linux/macOS, PowerShell/cmd on Windows);
  `cargo make doctor -- install [--dry-run]` installs the missing user-level
  tools (rustup/cargo/pip/pipx recipes and the pinned verification bundles;
  never sudo)
- `cargo make verify` / `verify-extract` / `verify-self-test` — Aeneas + Lean
  formal verification of every target registered in
  `verification/targets.toml`: re-extract Lean from the production Rust,
  reject drift against the committed `generated.lean`, build all proofs,
  audit axiom dependencies (including the per-target `sanity_*` theorems that
  keep each specification satisfiable on concrete inputs); `verify-extract`
  regenerates, `verify-self-test`
  additionally requires an injected source bug to be rejected by the proofs.
  Adjacent tasks: `verify-drift` (extraction diff only; also the pre-commit
  gate via `check.py drift --staged`), `verify-determinism` (byte-identical
  re-extraction), `verify-mutation-sweep` (generated mutants, kill-rate
  report), `verify-report` (trust-boundary report, no toolchain needed),
  `verify-premises` (run premise-evidence tests; renamed/moved/failing
  references fail), `verify-spec-diff` (advisory declaration diff of
  spec/proof Lean files), and `verify-new-target` (scaffold a registry entry,
  extraction module, and spec/proof skeletons with an authoring guide).
  `script/verification/check.py` accepts `--targets` / `--affected-by*` /
  `--diff` for incremental runs.
  Linux x86_64 only; tools via
  `cargo make doctor -- install --only formal-verification`. Changing a
  covered production function requires re-extracting and re-proving in the
  same commit

Shared-target invariant: crates consumed as path dependencies across group
workspaces must stay **rlib-only** (no `cdylib` in `[lib] crate-type`). A
cdylib crate-type makes cargo emit final artifacts under unhashed names
(`lib<name>.{rlib,so}`) in the shared `target/`; each group workspace then
overwrites the same file and downstream compiles fail with E0463 ("can't
find crate") in a self-perpetuating ping-pong. `sys_interface` and
`sys_interface_standalone` hit exactly this; both are rlib-only now. If the
`uniffi-bindings` workflow ever needs the cdylib back, isolate that build's
target dir instead of re-adding cdylib to the shared one.

## Before You Change Code

1. Read `ARCHITECTURE.md` for crate layering and dependency rules.
2. Run `cargo fmt` in each group workspace you touch.
3. Run `cargo clippy --workspace --all-targets -- -D warnings` per group before and after changes.
4. Run `cargo test --no-run --workspace` per group to ensure all test targets compile.
5. Or simply run `cargo make check` (`script/ci/check_all.py`), which executes
   steps 2–4 (in check mode) sequentially across all group/example workspaces.
6. When you change the mgen entity template or SQL in example procedure
   sources, run `cargo make check-sql` in the affected examples to validate
   SQL literals against `sql/ddl.sql` (see `doc/en/sql_subset.md`).

## Memory-Constrained Environments

`.cargo/config.toml` caps build/test memory so `cargo test` survives an ~8 GB
budget: `jobs = 4`, a rust-lld linker wrapper (`.cargo/cc-lld.sh`), and
`RUST_TEST_THREADS = 4`. This config applies to every group workspace through
directory inheritance, and its `target-dir = "target"` points all builds at
the shared repository-root `target/`. The CI setup actions
(`.github/actions/rust-*-setup`) export a fully pinned
`BINDGEN_EXTRA_CLANG_ARGS` (`-nostdinc` plus clang's resource dir and the
explicit C include dirs) so the bindgen runs of `rliburing` and `libsql-ffi`
(both C-only) stay deterministic on toolchain images that leak libstdc++'s
C++-only header directory onto libclang's search path. Ordering tricks such
as `-isystem`-first or `-nostdinc++` are not enough: clang's resource-dir
`<stdatomic.h>` delegates with `#include_next`, and the C++-only variant it
lands on declares nothing in C mode, so the build fails with `use of
undeclared identifier 'memory_order_*'` inside
`/usr/include/liburing/barrier.h`. Only `-nostdinc` removes the leaked
directory from the delegation chain. Each group workspace root `Cargo.toml`
also sets `profile.dev.debug = 1` (line tables only) to shrink linker memory.
Do not raise these limits without re-measuring peak memory.

## Formatting

- The entire workspace is formatted with `cargo fmt` using the default Rust style.
- The pre-commit hook (`.githooks/pre-commit` / `.githooks/pre-commit.ps1`) is a thin wrapper over `script/ci/check_all.py --only fmt,clippy,deny` (the fast deterministic per-group steps; `deny` covers bans/licenses/sources — the time-varying advisories check stays in CI), followed by a verification drift gate (`check.py drift --staged --skip-if-no-tools`, Linux x86_64 with the formal toolchain installed; skipped silently otherwise). Commits are blocked when a step fails; run `cargo make check` for the full suite.
- CI also enforces formatting with a dedicated `cargo-fmt` job that loops over all workspaces.

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
- Do not add a dev-dependency on the `mudu_kernel` facade for unit tests;
  depend on the narrow crate under test (or the `catalog-testing` fixture
  crate for meta-mgr fixtures). `command` is the one grandfathered exception: its
  fs-type tests need a `conn`-level fixture, and a direct `conn` dev-dep
  would violate the rank layering.
- Production dependencies (`[dependencies]` / `[build-dependencies]`) must
  never enable any crate's `test` feature; the fuzz workspace
  (`crates/db-kernel/mudu_kernel/fuzz`) is the only whitelisted exception.
  This is enforced by `script/ci/check_test_feature.py` (wired into
  `script/ci/check_all.py`). Share test-only helper code via a dedicated
  `*-testing` crate consumed through dev-dependencies (e.g.
  `catalog-testing`), or via the `testing` crate's public API. A crate's own
  `test` feature is reserved for what cannot move out: orphan-rule-bound
  items (`Arbitrary` impls) and fuzz targets.

## CI / Tooling

The following checks run in CI (see `.github/workflows/ci-hardening.yaml`);
each runs once per group workspace (`crates/common`, `crates/db-kernel`,
`crates/sdk`, `example/mpk`, `crates/tools`) because every group has its own
`Cargo.lock`:

- `cargo fmt -- --check`
- `cargo deny check --config <repo-root>/deny.toml bans licenses advisories sources`
- `cargo udeps --workspace --all-targets`
- `cargo hack check --workspace --each-feature --no-dev-deps` (non-blocking: pre-existing feature-interaction issues in external crates)
- `cargo outdated --workspace --root-deps-only` (non-blocking)

Repository-wide invariants also run in the `workspace-invariants` job:

- `python3 script/ci/sync_workspace_deps.py --check` — versions.toml is the
  single source of truth for shared external deps.
- `python3 script/ci/check_arch.py` — ARCHITECTURE.md crate layering plus
  dependency-cycle detection over the normal (non-dev/build) dependency graph.
- `cargo run -p mudu_metrics -- --root crates --config code_metrics.toml` —
  file / class / struct / function line limits (`crates/tools/dev/mudu_metrics`).
  `code_metrics.toml` configures the limits and the `exclude` list; generated
  files carrying the `Generated by ... Do not edit manually.` header are skipped.

When adding a new crate, always include a `license = "..."` field and prefer workspace dependencies.

## Language Bindings (AS / C# / Python / C / Go / Rust)

The bindings share one source of truth and one naming scheme; keep them that
way (full details: `doc/en/binding_api_surface.md`).

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
  module is `github.com/mududb/mududb-go` with packages
  `types`/`codec`; the C binding carries the canonical names literally as
  include paths (`mududb/types/Uni*.h`, `mududb/codec/mpack.h`) — identical
  subpaths.
- **Hand-written layer stays thin**: only facades (db/fs/sys entry points,
  mpack runtime, C# mock backend) plus the per-language record bridges for
  user-defined procedure types (`--type-wit`; AS `assembly/record.ts`, C#
  `mudu_sys/RecordBridge.cs`, py `codec/bridge.py`, C
  `codec/record_bridge.{h,c}`, Go `types/bridge.go` — authoring guide:
  `doc/en/custom_types.md`) are hand-written, following the canonical
  surface table in `doc/en/binding_api_surface.md`. Update that document
  first when adding a facade capability. The facade exists in all four
  guest languages: AS (reference), C#, Rust (`mududb::db/fs/result`), and
  Python (`mududb.db/sql/result/fs/sys`, running in componentize-py
  guests — pipeline and example: `example/mpk/wallet-py`; mtp front-end:
  `mudu_transpiler`'s `python` subcommand).
- **Consistency gates**: `python3 script/ci/check_bindings_regen.py` (regen
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

### Terminology glossary

- Follow the organizational convention of the [PostgreSQL Glossary](https://www.postgresql.org/docs/current/glossary.html): alphabetized entries, no sequential entry numbers, and named cross-references. This governs presentation, not MuduDB's definitions.
- `doc/en/terminology.md` is canonical; update `doc/zh-CN/terminology.zh-CN.md` in the same change. Keep definitions, explicit anchors, and entry order aligned. Chinese definition headings include the canonical English name.
- Write displayed English term names with spaces instead of hyphens and capitalize the first letter of every word, for example `Control Plane Client` and `Single Replica Set Deployment`. Preserve acronyms (`LSN`, `HA`) and established internal capitalization (`RaftFollower`). Apply this spelling to headings, indexes, and named term references; do not change URL anchors, code identifiers, configuration keys, or ordinary prose merely to match display typography.
- Sort both the index and definitions by canonical English title, case-insensitively (spaces retain their literal ordering). Re-sort after display-name changes. The Chinese version follows that order, not pinyin. Keep architecture explanations outside the alphabetical definitions.
- Do not introduce or restore `Txx` entry numbers. Cite a term by name with its explicit semantic anchor, such as `terminology.md#partition`; preserve existing anchors when reordering or renaming entries, including `#lsn` and `#epoch`.
- Define each concept once. Give abbreviations and accepted aliases alphabetized “See” / “参见” entries linking to the canonical definition; distinguish these from retired names that must not be treated as aliases.
- When reorganizing terminology, inspect repository references and update affected links and wording in the same change. Verify alphabetical order, bilingual entry/anchor parity, unique anchors, and glossary link targets. Documentation-only changes do not require Rust build checks.
