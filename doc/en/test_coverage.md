# MuduDB Test Coverage Guide

This guide explains how to collect and view MuduDB code coverage locally and in CI, and how to improve coverage by adding tests.

## 1. Coverage tooling

MuduDB uses [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) to generate reports based on LLVM source-based coverage. Compared with sampling-based tools, it maps async/await, proc-macros, and workspace projects more accurately.

- **Line coverage**: statement/line coverage.
- **Function coverage**: function-level coverage.
- **Branch coverage**: branch coverage (nightly-only unstable feature, **disabled** by default — the pinned nightly's LLVM can SIGSEGV on branch data at report time; `--branch` opts in).

## 2. Prerequisites

The repository pins a nightly toolchain such as `nightly-2026-06-17` (see `.rust-nightly-version`). Before running coverage you need:

1. `rustup` installed.
2. The pinned nightly toolchain (the script installs it automatically).
3. `llvm-tools-preview` and `cargo-llvm-cov` (the script installs them automatically).

If you have already run `script/setup_dev_env.sh`, you only need to run the coverage script.

## 3. One-command coverage

```bash
cargo make coverage
```

This delegates to the cross-platform Python runner (works on Linux / macOS / Windows); direct invocation is equivalent:

```bash
python3 script/coverage/run_coverage.py
```

Defaults:

- Scope: unit tests of **all five group workspaces** (`crates/common`, `crates/db-kernel`, `crates/sdk`, `example/mpk`, `crates/tools`) **plus the `testing` crate's end-to-end tests**.
- Report: **one merged report** over everything that ran (see "How merging works" below).
- Format: `all` (HTML, JSON, and lcov).
- Output directory: `target/llvm-cov` (`html/index.html`, `coverage.json`, `coverage.lcov`).
- Branch coverage: **disabled** (`--branch` opts in; unstable, see §8.2).
- Unit tests run with `--test-threads=1` to avoid non-determinism; e2e tests keep the default parallelism (like `cargo make test`).

### How merging works

All group workspaces share the repository-root `target/` directory, so every profraw file lands flat in `target/llvm-cov-target/`. The report step merges them with the pinned nightly's `llvm-profdata` and renders a single report via `llvm-cov show/export` over exactly the instrumented test binaries that ran (`cargo llvm-cov report` itself cannot merge across workspaces).

### e2e coverage

The `testing` crate's integration tests run a full server **in-process**, so e2e coverage needs no special instrumentation: the test binaries' profraw is collected like any other. Before the e2e stage the script builds `wallet.mpk` on a best-effort basis (wallet-dependent tests soft-skip if it is missing); the committed fixture under `crates/db-kernel/testing/mpk/` is restored to its pre-run bytes afterwards, so a coverage run never dirties the working tree (a deliberate fixture refresh is committed via `cargo make copy-wallet-mpk` instead). Use `--no-e2e` to skip e2e entirely, or `--e2e-filter <substring>` for a quick smoke run.

### 3.1 Common options

```bash
# Unit tests only (much faster)
python3 script/coverage/run_coverage.py --no-e2e

# Run only e2e tests matching a substring (smoke)
python3 script/coverage/run_coverage.py --e2e-filter string_literal

# HTML only
python3 script/coverage/run_coverage.py -f html

# Force branch coverage (unstable, may crash)
python3 script/coverage/run_coverage.py --branch

# Custom output directory
python3 script/coverage/run_coverage.py -o /tmp/mudu-cov
```

For the full option list:

```bash
python3 script/coverage/run_coverage.py --help
```

## 4. Viewing reports

### 4.1 HTML report

After generation, open:

```bash
xdg-open target/llvm-cov/html/index.html
```

The report shows line, function, and branch coverage per file and highlights uncovered code.

### 4.2 JSON summary

`target/llvm-cov/coverage.json` contains the overall summary:

```bash
python3 - <<'PY'
import json
with open('target/llvm-cov/coverage.json') as f:
    data = json.load(f)
totals = data['data'][0]['totals']
for key in ('lines', 'functions', 'branches'):
    print(f"{key}: {totals[key]['percent']:.2f}%")
PY
```

### 4.3 lcov

`target/llvm-cov/coverage.lcov` can be imported into lcov-compatible tools (e.g. VS Code extensions, genhtml).

## 5. Manual commands

The script's pipeline, if you need finer control:

```bash
NIGHTLY=nightly-2026-06-17
LLVM_BIN="$(rustc +"${NIGHTLY}" --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin"

# 1. Collect per group workspace (testing is excluded from the unit stage)
(cd crates/common    && cargo +"${NIGHTLY}" llvm-cov --workspace --lib --tests --no-report -- --test-threads=1)
(cd crates/sdk       && cargo +"${NIGHTLY}" llvm-cov --workspace --lib --tests --no-report -- --test-threads=1)
(cd crates/tools     && cargo +"${NIGHTLY}" llvm-cov --workspace --lib --tests --no-report -- --test-threads=1)
(cd crates/db-kernel && cargo +"${NIGHTLY}" llvm-cov --workspace --exclude testing --lib --tests --no-report -- --test-threads=1)
(cd crates/db-kernel && cargo +"${NIGHTLY}" llvm-cov --package testing --lib --tests --no-report)  # e2e stage

# 2. Merge every profraw (they all land flat in target/llvm-cov-target/)
"${LLVM_BIN}/llvm-profdata" merge -sparse target/llvm-cov-target/*.profraw -o target/llvm-cov/merged.profdata

# 3. Render one report over exactly the test binaries that ran
"${LLVM_BIN}/llvm-cov" show   --instr-profile=target/llvm-cov/merged.profdata <test-binary> [-object <test-binary>...] -format=html --output-dir=target/llvm-cov/html
"${LLVM_BIN}/llvm-cov" export --instr-profile=target/llvm-cov/merged.profdata <test-binary> [-object <test-binary>...] -format=text > target/llvm-cov/coverage.json
"${LLVM_BIN}/llvm-cov" export --instr-profile=target/llvm-cov/merged.profdata <test-binary> [-object <test-binary>...] -format=lcov > target/llvm-cov/coverage.lcov
```

The exact object list comes from `cargo test --no-run --message-format=json` run with the environment printed by `cargo +"${NIGHTLY}" llvm-cov show-env --export-prefix` (plus `CARGO_TARGET_DIR=target/llvm-cov-target`); see `script/coverage/run_coverage.py` for the full details.

## 6. Coverage in CI

The `.github/workflows/nightly-checks.yaml` workflow contains a `coverage` job:

- Runs merged unit-test coverage every night (e2e stays local-only: `--no-e2e`).
- Generates HTML / JSON / lcov and uploads them as artifacts (retained for 30 days).
- Posts a coverage summary to the job summary page.

When the CI job fails, the local reproduction command is printed in the logs.

## 7. Improving coverage by adding tests

Add a new test file under the crate's `src/` directory, named `*_test.rs`, and include it in `mod.rs` or `lib.rs`:

```rust
#[cfg(test)]
mod my_feature_test;
```

Priority areas for new tests:

- Utility functions with many conditional branches (e.g. `case_convert`, `xid`, `result_of`).
- Type conversions and boundary checks (e.g. `scalar_type`, tuple binary/json conversion).
- Core public APIs currently showing 0% coverage.

After running coverage, use the HTML report to locate uncovered code and add targeted tests.

## 8. Troubleshooting

### 8.1 Branch coverage shows `0/0`

The `--branch` flag must be supplied in **both** the test-collection phase and the report phase. If it is only passed during test collection but omitted from `report`, the branch column will show `0/0`. The script and CI handle this automatically; check both commands when running manually.

### 8.2 Report generation crashes (SIGSEGV)

On the current pinned nightly, branch coverage data for the kernel crates can trigger an LLVM bug that SIGSEGVs report generation (`getInstantiationGroups`). Branch coverage is therefore **disabled by default**; only opt in with `--branch` if you need it:

```bash
python3 script/coverage/run_coverage.py --branch
```

`mudu`, `mudu_type`, and `mudu_contract` work fine with `--branch` both individually and combined.

### 8.3 `rustup` download fails

The script retries `rustup` commands up to three times. If it still fails, clean up and retry:

```bash
rm -rf ~/.rustup/downloads/*.partial
rustup toolchain uninstall nightly-2026-06-17 2>/dev/null || true
python3 script/coverage/run_coverage.py
```

### 8.4 First run is slow

The first run installs the pinned nightly, `llvm-tools-preview`, and `cargo-llvm-cov`, and recompiles dependencies, which can take several minutes. Subsequent runs reuse the installed tools and build cache.

## 9. Related files

- `script/coverage/run_coverage.py`: local coverage entry point (cross-platform Python, also reachable via `cargo make coverage`).
- `script/coverage/install_deps.py`: coverage toolchain dependency installer (invoked automatically by run_coverage.py).
- `.github/workflows/nightly-checks.yaml`: CI coverage job.
- `.github/scripts/summarize_coverage.py`: coverage summary script.
