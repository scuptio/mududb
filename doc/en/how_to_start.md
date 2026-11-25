# How to Start

Use this page to prepare a development environment. This guide covers two ways to get started:

- **[Develop MuduDB from source](#develop-mududb-from-source)** – build from source with a reproducible, pinned toolchain (recommended). The source path offers three setup options: start with option A, then follow [Your first MPK](your_first_mpk.md) to build, install and invoke the wallet example. Options B and C are alternative setup methods; you do not need to run all three.
- **[Use MuduDB with `mudup`](#use-mududb-with-mudup)** – install released binaries with `mudup` (planned; not available yet).

---

## Develop MuduDB from source

### Clone the repository

```bash
git clone https://github.com/scuptio/mududb.git
cd mududb
```

### Option A: One-command setup (recommended)

On Ubuntu/Debian, run the provided setup script. It installs system packages, rustup, the pinned stable and nightly Rust toolchains, a local Python virtual environment, and `cargo-make`.

```bash
./script/setup_dev_env.sh
```

After the script finishes, activate the Python virtual environment:

```bash
source .venv/bin/activate
```

Then verify the toolchain — this reports every tool MuduDB's build, examples, and release flows need, with per-OS install commands for anything missing:

```bash
cargo make doctor
```

Then verify the build. The repository is split into independent Cargo workspaces: four under `crates/` (`common`, `db-kernel`, `sdk`, `tools`) plus `example/mpk`; build each workspace from its own directory:

```bash
cd crates/db-kernel
cargo build
cargo test
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
```

Or run the full check sequence for all five group workspaces from the repository root:

```bash
cargo make check
```

Run all workspace tests with `cargo make test`. Test failures do not stop later
test targets or workspaces; the command returns a nonzero exit code if any group
fails. Each run writes complete per-workspace logs plus `summary.txt` and
`summary.json` under `logs/tests/<timestamp>-<unique-id>/`. The report includes
failed test names and targets, diagnostic excerpts, exit codes and totals for
passed/failed/ignored tests and failed workspaces. `failures.log` combines the
complete output of every failed workspace without truncation. Compilation errors
and process failures without a test count are listed separately.
Compilation failures may prevent a workspace's tests from running, but later
workspaces are still attempted. Ctrl+C stops the run and preserves partial results.

Use `cargo make test -- --fail-fast` for early exit, or
`cargo make test -- --group db-kernel` for one workspace. The `test-group-*`
tasks use the same logging runner. `--log-dir <path>` selects the parent directory
for run logs; every invocation creates a new directory without overwriting old logs.
Terminal output highlights progress, failures and a final workspace results table
with elapsed times and clickable report links. Use `--color always` or
`--color never` to override automatic terminal detection; `NO_COLOR` disables
colors in auto mode. Log files and saved reports remain plain text.

### Option B: Dev Container / Docker

If you prefer a containerized environment, build and run the development image:

```bash
docker build -f Dockerfile.dev -t mududb-dev .
docker run -it --rm -v "$(pwd):/workspace" -v mududb-dev-target:/workspace/target mududb-dev
```

Or open the project in an editor that supports [Dev Containers](https://containers.dev/) using `.devcontainer/devcontainer.json`.

Container builds must not share the host `target/`. The development image uses
a Docker volume at `/workspace/target`; the command above, Dev Containers, and
the distributed E2E driver each use separate named caches. Rebuild existing
development images and recreate containers after changing this configuration.
The E2E image build uses its own `/opt/mududb-e2e-target`; local `act` runs use
copied workspaces (`--bind=false`).

E2E builds require BuildKit. Their target and Cargo download caches are separate
BuildKit caches, partitioned by architecture and locked during builds. The
server binary is exported outside the cache before entering the runtime image.
The driver uses external volumes `e2e_driver-target`, `e2e_driver-registry`, and
`e2e_driver-git`. The test harness creates these automatically; `down --volumes`
removes scenario data but preserves these caches. For direct Compose use, create
the three volumes with `docker volume create <name>` first. Override their names
with `MUDUDB_E2E_TARGET_VOLUME`, `MUDUDB_E2E_REGISTRY_VOLUME`, and
`MUDUDB_E2E_GIT_VOLUME` when separate checkouts need separate caches. Existing
drivers acquire the download-cache mounts on their next recreation. Cache
cleanup is manual: remove only unused named cache volumes; BuildKit cache
pruning also discards incremental image-build artifacts.

### Option C: Manual setup

If you cannot use the script or container, follow these steps.

#### 1. Install system packages

```bash
sudo apt-get update -y
sudo apt-get install -y \
    python3 python3-pip python3-venv python-is-python3 \
    build-essential curl liburing-dev \
    clang libclang-dev llvm-dev pkgconf
```

Package purpose:

- `python3`, `python3-pip`, `python3-venv`: isolated Python environment for example build scripts.
- `build-essential`, `curl`: native compilation and rustup installation.
- `liburing-dev`: Linux `io_uring` backend used by `mudu_kernel`.
- `clang`, `libclang-dev`, `llvm-dev`: used by [bindgen](https://github.com/rust-lang/rust-bindgen).

#### 2. Install Rust toolchains

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Inside the repository, `rust-toolchain.toml` pins the stable toolchain and components (`clippy`, `rustfmt`, targets). `cargo build`, `cargo test`, `cargo clippy`, and `cargo doc` automatically use it.

For auxiliary checks that require nightly, install the pinned nightly:

```bash
NIGHTLY_TOOLCHAIN="$(tr -d '[:space:]' < .rust-nightly-version)"
rustup toolchain install "$NIGHTLY_TOOLCHAIN" --profile minimal
```

#### 3. Set up the Python virtual environment

```bash
python3 -m venv .venv
source .venv/bin/activate
python -m pip install --upgrade pip
```

The transpiler uses Python 3.11+ and the standard library; no pip packages are required.

Reactivate the environment in any new shell with `source .venv/bin/activate`.

#### 4. Install `cargo-make`

```bash
cargo install cargo-make
```

---

## Install development binaries

After the environment is ready, install the development build of the tools and server:

```bash
python script/build/install_binaries.py
```

Default installed tools:

- `mpm`: portable project builder
- `mpm-build`: package builder
- `mpm-crate`: project scaffolder
- `mpm-install`: package installer
- `mpm-tool`: package inspector/verifier/converter
- `mgen`: source generator
- `mtp`: transpiler
- `mudud`: MuduDB server
- `mcli`: TCP protocol client CLI

To install every workspace binary target instead:

```bash
python script/build/install_binaries.py --all-workspace-bins
```

---

## Use MuduDB with `mudup`

> **Note:** `mudup` is a planned installer whose design is still settling — see the [design proposal](mudup.md). The commands below are not available yet. The working install path today is building from source with `cargo make`, or running `python script/build/install_binaries.py` as described in [Install development binaries](#install-development-binaries).

Once released, `mudup` will install the release artifacts for server deployment or daily use.

### 1. Install `mudup`

```bash
curl --proto '=https' --tlsv1.2 -fsSL https://github.com/scuptio/mudup/releases/download/latest/mudup-init.sh | sh
mudup --help
```

### 2. Install MuduDB and its toolchain

```bash
mudup install
```

This will install and activate the latest release binaries: `mpm`, `mpm-build`, `mpm-crate`, `mpm-install`, `mpm-tool`, `mgen`, `mtp`, `mudud`, `mcli`.

### 3. Verify installation

```bash
mudud --version
mcli --version
```

Then create a configuration file and start the server as described in [Running mudud](running_mudud.md).

---

## Toolchain policy

- **Stable Rust** (pinned in `rust-toolchain.toml`) is the official toolchain for `cargo build`, `cargo check`, `cargo test`, `cargo clippy`, `cargo doc`, and release artifacts.
- **Pinned nightly** (in `.rust-nightly-version`) is used only for auxiliary checks that genuinely require nightly: `cargo fuzz`, `cargo-udeps`, Miri, and sanitizers.
- Do **not** use `RUSTC_BOOTSTRAP` to make nightly features available to stable builds.
- Product code must not introduce new nightly features. If one is unavoidable, it requires architecture review, documented alternative analysis, impact scope, and an exit plan.

## Running auxiliary checks locally

```bash
NIGHTLY_TOOLCHAIN="$(tr -d '[:space:]' < .rust-nightly-version)"

# Code coverage (see doc/en/test_coverage.md)
python3 script/coverage/run_coverage.py

# cargo-udeps
cargo +"${NIGHTLY_TOOLCHAIN}" install cargo-udeps --version 0.1.61 --locked
cargo +"${NIGHTLY_TOOLCHAIN}" udeps --workspace --all-targets

# cargo-fuzz
cd mudu_kernel/fuzz
cargo +"${NIGHTLY_TOOLCHAIN}" install cargo-fuzz --version 0.13.2 --locked
cargo +"${NIGHTLY_TOOLCHAIN}" fuzz run <target> -- -max_total_time=60

# Miri
rustup component add miri --toolchain "${NIGHTLY_TOOLCHAIN}"
cargo +"${NIGHTLY_TOOLCHAIN}" miri test --workspace

# AddressSanitizer
RUSTFLAGS="-Z sanitizer=address" \
  LSAN_OPTIONS="suppressions=$(pwd)/script/ci/lsan_suppressions.txt" \
  cargo +"${NIGHTLY_TOOLCHAIN}" test --workspace --lib --tests --bins --target x86_64-unknown-linux-gnu
```

## Toolchain upgrades

Stable and nightly toolchain upgrades must be made through **separate pull requests**:

- **Stable upgrade**: modify `rust-toolchain.toml` (and `workspace.package.rust-version` in `Cargo.toml` if the MSRV changes). Run the full compatibility test suite and benchmarks before merging.
- **Nightly upgrade**: modify `.rust-nightly-version`. Run all nightly-only checks (`cargo-udeps`, `cargo fuzz`, Miri, sanitizers) before merging.

Both upgrades are explicit commits; no floating toolchain channels are used in CI or release workflows.

---

## Configuration file

Generate a default `./mudud.cfg` in the current directory without starting the server:

```bash
mudud init-cfg
```

`mudud` loads the first configuration file it finds and does not create one automatically. The full search order, configuration fields and custom `--cfg` / `-c` paths are documented in [Running mudud](running_mudud.md).

---

## Use MuduDB

Optional reading: [`mcli` Management Interface (HTTP)](./mcli_admin.md).

### 1. Start `mudud`

Make sure the server has a high open-files limit. A low soft `nofile` limit such as `1024` can cause stalls or failed session setup under higher connection counts.

For a shell-launched local server:

```bash
ulimit -n 65535
mudud
```

If `mudud` is launched by `systemd` or another supervisor, configure the service-level limit, for example `LimitNOFILE=65535`.

Verify the live limit after startup:

```bash
cat /proc/$(pgrep -x mudud)/limits | grep 'open files'
```

### 2. Use `mcli` interactive shell for CRUD

```bash
mcli --addr 127.0.0.1:9527 shell --app demo
```

Run a complete CRUD flow:

```sql
CREATE TABLE users_demo (
  id INT PRIMARY KEY,
  name TEXT
);

INSERT INTO users_demo (id, name) VALUES (1, 'Alice');
SELECT id, name FROM users_demo WHERE id = 1;

UPDATE users_demo SET name = 'Alice-Updated' WHERE id = 1;
SELECT id, name FROM users_demo WHERE id = 1;

DELETE FROM users_demo WHERE id = 1;
SELECT id, name FROM users_demo;
```

Exit shell:

```text
\q
```

Shell notes:

- End each SQL statement with `;`.
- Meta commands: `\q`, `\help`, `\app <name>`.
- Query results are shown in an interactive table on TTY by default.

### 3. Build, install, and use the wallet app

Follow [Your first MPK](your_first_mpk.md) for the package path, configuration, installation commands and expected results. Start at the repository root so that the shared `target/` path resolves correctly.

## Troubleshooting / FAQ

### `mudud` fails to start with an io_uring error

MuduDB requires Linux kernel 5.1+ with `CONFIG_IO_URING=y`. Check:

```bash
uname -r                                # must be >= 5.1
bash script/build/inspect_iouring_env.sh
```

The inspect script reports kernel toggles, seccomp status and container settings. `io_uring` is not a filesystem, so it cannot be detected through `/proc/filesystems`; see [DEPLOY.md](DEPLOY.md) for environment details.

If you are running inside Docker, use `--privileged` or add `--cap-add CAP_SYS_ADMIN --ulimit memlock=-1:-1`.

### `mudud` starts but sessions hang or fail to establish

Raise the open-file limit before starting `mudud`:

```bash
ulimit -n 65535
mudud
```

If `mudud` is managed by `systemd`, set `LimitNOFILE=65535` in the service file.

### `cargo build` fails with “target wasm32-wasip2 is not installed”

The stable toolchain pinned in `rust-toolchain.toml` should include the target, but you can also install it explicitly:

```bash
rustup target add wasm32-wasip2
```

### `cargo build` fails with missing `liburing` headers

Install the system dependency:

```bash
sudo apt-get install liburing-dev
```

### `mcli` cannot connect to the server

- The TCP protocol port is `9527` by default (`--addr 127.0.0.1:9527`).
- The HTTP management port is `8300` by default (`--http-addr 127.0.0.1:8300`).
- Some commands such as `app-invoke` require both TCP and HTTP endpoints.
- Make sure `mudud` is running and no firewall is blocking the ports.

### `app-invoke` says the app or procedure is not found

Install the MPK first:

```bash
mcli --http-addr 127.0.0.1:8300 app-install --mpk path/to/package.mpk
```

Then verify the names with:

```bash
mcli --http-addr 127.0.0.1:8300 app-list
mcli --http-addr 127.0.0.1:8300 app-detail --app <app>
```

### Where can I learn more?

- Core concepts: [`concepts.md`](concepts.md)
- Deployment scripts: [`DEPLOY.md`](DEPLOY.md)
- Documentation index: [`../README.md`](../README.md)

---

## Contributing

Before submitting changes:

1. Ensure `cargo fmt`, `cargo clippy`, and `cargo test` pass on the pinned stable toolchain.
2. Keep changes focused and follow the existing code style.
3. Update relevant documentation if your change affects build, configuration, or public behavior.

For the full development setup, see the sections above.

### Reports for long-running tasks

`check` (including `check-*`), `udeps`, `dep-audit`, `outdated`, `bench-smoke`
(including its per-benchmark tasks), `build-sdk`, `package-examples`, and
`test-e2e-distributed` continue independent steps after a failure and report
all outcomes. They run serially to preserve the memory budget and target isolation.
Use `cargo make <task> -- --help` for options: `--fail-fast`,
`--log-dir <directory>`, and `--color auto|always|never`.

Each run saves `summary.txt`, `summary.json`, plain per-step logs, and the complete
failed/advisory output in `failures.log`, under `logs/<task-family>/<unique-run>/`.
The summary distinguishes passed, failed, skipped, advisory, and interrupted steps,
with exit codes, diagnostic excerpts, durations, and full log paths. Required failures
return nonzero after independent work finishes; interruption returns 130.

- `outdated` remains advisory: its errors are recorded but alone do not fail the batch.
- Distributed e2e saves Compose node logs and cleans up after each scenario. Failed
  image preparation blocks scenarios; failed cleanup blocks subsequent scenarios.
- Benchmark execution and comparison remain dependent; smoke runs never update baselines.
  Explicit `bench-baseline-update*` tasks retain strict failure handling.
- Example packaging installs shared tools once, then runs each language independently.
- SDK builds check bindings once and build the requested adapter runtime once. Language
  prerequisites and validation run independently. Successful language artifacts are retained;
  a partially failed batch has no publishable `manifest.json`.
- Pre-commit hooks explicitly use `--fail-fast` to keep the existing fast feedback.
