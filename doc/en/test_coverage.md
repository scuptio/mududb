# MuduDB Test Coverage Guide

This guide explains how to collect and view MuduDB code coverage locally and in CI, and how to improve coverage by adding tests.

## 1. Coverage tooling

MuduDB uses [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) to generate reports based on LLVM source-based coverage. Compared with sampling-based tools, it maps async/await, proc-macros, and workspace projects more accurately.

- **Line coverage**: statement/line coverage.
- **Function coverage**: function-level coverage.
- **Branch coverage**: branch coverage (nightly-only unstable feature, enabled by default, can be disabled).

## 2. Prerequisites

The repository pins a nightly toolchain such as `nightly-2026-06-17` (see `.rust-nightly-version`). Before running coverage you need:

1. `rustup` installed.
2. The pinned nightly toolchain (the script installs it automatically).
3. `llvm-tools-preview` and `cargo-llvm-cov` (the script installs them automatically).

If you have already run `script/setup_dev_env.sh`, you only need to run the coverage script.

## 3. One-command coverage

```bash
script/coverage/run_coverage.sh
```

Defaults:

- Scope: `core` profile, i.e. `mudu`, `mudu_type`, `mudu_contract`, and `mudu_kernel`.
- Format: `all` (HTML, JSON, and lcov).
- Output directory: `target/llvm-cov`.
- Branch coverage: **enabled** (`--branch`).
- Test threads: fixed at `--test-threads=1` to avoid non-determinism.

> The `workspace` profile defaults to line coverage (`--no-branch`) because the current pinned nightly triggers an LLVM SIGSEGV when generating a merged report that includes `mudu_kernel`. Use `--branch` to force it on.

### 3.1 Common options

```bash
# Disable branch coverage
script/coverage/run_coverage.sh --no-branch

# Full workspace, HTML only (line coverage by default)
script/coverage/run_coverage.sh -p workspace -f html

# Full workspace, force branch coverage (may crash)
script/coverage/run_coverage.sh -p workspace --branch

# Core crates, JSON summary only
script/coverage/run_coverage.sh -p core -f json

# Custom output directory
script/coverage/run_coverage.sh -o /tmp/mudu-cov
```

For the full option list:

```bash
script/coverage/run_coverage.sh --help
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

If you need finer control, run `cargo llvm-cov` directly.

### 5.1 Basic workflow

`cargo llvm-cov` cannot use `--output-path` more than once in a single command, so multi-format reports require two steps: first run tests with `--no-report` to collect raw data, then use `cargo llvm-cov report` to generate each format.

```bash
export NIGHTLY_TOOLCHAIN="nightly-2026-06-17"
export CARGO_INCREMENTAL=0

# Step 1: collect coverage data
cargo +"${NIGHTLY_TOOLCHAIN}" llvm-cov \
  --package mudu \
  --package mudu_type \
  --package mudu_contract \
  --package mudu_kernel \
  --lib --tests \
  --no-report \
  --branch \
  -- \
  --test-threads=1

# Step 2: generate reports
cargo +"${NIGHTLY_TOOLCHAIN}" llvm-cov report \
  --branch \
  --html --output-dir ./target/llvm-cov/html \
  --json --output-path ./target/llvm-cov/coverage.json \
  --lcov --output-path ./target/llvm-cov/coverage.lcov
```

### 5.2 Disabling branch coverage

Remove all `--branch` flags. The branch column in the report will then show `0/0`.

### 5.3 Generating a single format

```bash
# HTML only
cargo +"${NIGHTLY_TOOLCHAIN}" llvm-cov \
  --package mudu --package mudu_type --package mudu_contract --package mudu_kernel \
  --lib --tests --branch \
  --html --output-dir ./target/llvm-cov/html \
  -- --test-threads=1

# JSON only
cargo +"${NIGHTLY_TOOLCHAIN}" llvm-cov \
  --package mudu --package mudu_type --package mudu_contract --package mudu_kernel \
  --lib --tests --branch \
  --json --output-path ./target/llvm-cov/coverage.json \
  -- --test-threads=1
```

## 6. Coverage in CI

The `.github/workflows/nightly-checks.yaml` workflow contains a `coverage` job:

- Runs coverage for the `core` profile every night.
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

### 8.2 `llvm-cov export` crashes when running the `core` profile

On the current pinned nightly, `mudu_kernel`'s branch coverage data can trigger an LLVM bug that causes `llvm-cov export` to SIGSEGV. Disable branch coverage in that case:

```bash
script/coverage/run_coverage.sh --no-branch
```

`mudu`, `mudu_type`, and `mudu_contract` work fine with `--branch` both individually and combined.

### 8.3 `rustup` download fails

The script retries `rustup` commands up to three times. If it still fails, clean up and retry:

```bash
rm -rf ~/.rustup/downloads/*.partial
rustup toolchain uninstall nightly-2026-06-17 2>/dev/null || true
script/coverage/run_coverage.sh
```

### 8.4 First run is slow

The first run installs the pinned nightly, `llvm-tools-preview`, and `cargo-llvm-cov`, and recompiles dependencies, which can take several minutes. Subsequent runs reuse the installed tools and build cache.

## 9. Related files

- `script/coverage/run_coverage.sh`: local coverage entry point.
- `.github/workflows/nightly-checks.yaml`: CI coverage job.
- `.github/scripts/summarize_coverage.py`: coverage summary script.
- `doc/cn/todo/coverage-tracking.md`: coverage mechanism design document (Chinese).
