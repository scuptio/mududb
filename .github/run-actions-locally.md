# Run GitHub Actions locally with `act`

This guide shows how to run the `mududb` GitHub Actions workflows on your own machine using [`act`](https://nektosact.com/introduction.html). This is useful for:

- Validating CI changes before pushing.
- Running the full build/test/clippy pipeline without consuming GitHub runner minutes.
- Reproducing failures that only show up in CI.

> **What is `act`?**  
> `act` is a command-line tool that runs GitHub Actions workflows locally inside Docker containers. It reads `.github/workflows/*.yaml` and replicates the GitHub Actions runner environment as closely as possible.

---

## Table of contents

1. [Quick start](#quick-start)
2. [Prerequisites](#prerequisites)
3. [Why we need a custom `.actrc`](#why-we-need-a-custom-actrc)
4. [Running workflows](#running-workflows)
5. [Troubleshooting](#troubleshooting)
6. [Reference](#reference)

---

## Quick start

If you already have Docker and `act` installed, run the main build workflow with:

```bash
act -W .github/workflows/build.yaml -j build
```

This command:

- Uses the settings in `.actrc`.
- Runs the `build` job from `.github/workflows/build.yaml`.
- Reuses the local Docker image by default (see [Why we need a custom `.actrc`](#why-we-need-a-custom-actrc)).

To verify everything is wired correctly first, run a dry check:

```bash
act -W .github/workflows/build.yaml -j build --dryrun
```

---

## Prerequisites

### 1. Docker

Docker must be installed and running. See the [Docker installation guide](https://docs.docker.com/engine/install).

Verify:

```bash
docker --version
docker info
```

### 2. `act`

Install `act` into a directory on your `PATH` (example: `$HOME/.local/bin`):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/nektos/act/master/install.sh | bash -s -- -b "$HOME/.local/bin"
```

For other installation options, see the [official `act` documentation](https://nektosact.com/installation/index.html).

Verify:

```bash
act --version
```

### 3. Linux for `io_uring` tests

The `io_uring`-based storage paths require Linux. macOS and Windows hosts can still run `act`, but the `io_uring` tests will not exercise the real kernel path.

Recommended host kernel: **Linux >= 5.10**.

Check your host:

```bash
uname -r
cat /proc/sys/kernel/io_uring_disabled
```

Expected:

- Kernel version `5.10` or newer.
- `io_uring_disabled` is `0`.

---

## Why we need a custom `.actrc`

This repository contains a `.actrc` file in the project root. `act` automatically reads it on every run, so you usually do **not** need to pass these flags manually.

```text
--container-architecture linux/amd64
-P ubuntu-latest=ghcr.io/catthehacker/ubuntu:act-latest
--pull=false
--action-offline-mode
--privileged
--container-options --security-opt=seccomp=unconfined
--container-options --security-opt=apparmor=unconfined
--env ACT=true
```

What each option does:

| Option | Purpose |
|--------|---------|
| `--container-architecture linux/amd64` | Force the container image architecture. |
| `-P ubuntu-latest=ghcr.io/catthehacker/ubuntu:act-latest` | Pin the runner image to the community `act` image. |
| `--pull=false` | Do not pull the image every run. Set to `true` for the first run or when you want to update. |
| `--action-offline-mode` | Avoid fetching action metadata from GitHub on every run. |
| `--privileged` | Grant the container the privileges required by `io_uring`. |
| `--container-options --security-opt=seccomp=unconfined` | Disable seccomp filtering so `io_uring` syscalls are allowed. |
| `--container-options --security-opt=apparmor=unconfined` | Disable AppArmor so `io_uring` can initialize. |
| `--env ACT=true` | Expose `env.ACT` so workflows can skip CI-only steps (e.g. disk cleanup) during local runs. |

> **Important:** The `--privileged` and `--security-opt` flags are required for the `io_uring` tests. Without them, you will see errors such as `io_uring_queue_init_params error -1`.

If you override flags on the command line, `act` may stop reading `.actrc`. In that case, pass the settings explicitly or rely on the defaults in the file for normal runs.

---

## Running workflows

### Helper script

The repository provides `script/ci/run-local.sh`, which wraps `act` and captures logs for each workflow or job in a single place. It is the easiest way to run actions locally without losing errors in long output.

```bash
# Run the main build workflow and capture a summary
./script/ci/run-local.sh

# Run a specific workflow
./script/ci/run-local.sh .github/workflows/nightly-checks.yaml

# Run one or more jobs individually
./script/ci/run-local.sh -j cargo-deny
./script/ci/run-local.sh -j build -j rustdoc

# Show help
./script/ci/run-local.sh --help
```

For each run, the script writes a log file under `logs/` and prints a final summary. If anything failed, it extracts the last matching error lines so you do not have to scroll through the full `act` output.

### Log files from workflow steps

The CI workflows also write per-step log files inside the container. When `act` uses `--bind` (the default in many setups), these files appear in your local checkout at:

- `logs/build-*.log` — `build.yaml` steps (check, clippy, test, etc.)
- `logs/hardening-*.log` — `ci-hardening.yaml` steps (fmt, deny, hack, outdated)
- `logs/nightly-*.log` — `nightly-checks.yaml` steps (udeps, fuzz, miri, asan, coverage)
- `logs/compat-*.log` — `compatibility.yaml` steps

Because each heavy step uses `set -o pipefail | tee`, the original exit code is preserved and the log file survives after the container exits.

### Main build workflow

Pull the image and run the first time:

```bash
act -W .github/workflows/build.yaml -j build --pull=true
```

Run normally (reuses the cached image):

```bash
act -W .github/workflows/build.yaml -j build
```

Run with verbose logging:

```bash
act -W .github/workflows/build.yaml -j build -v
```

Reuse the same container between runs (faster, but only after security settings are confirmed good):

```bash
act -W .github/workflows/build.yaml -j build --reuse
```

### Build workflow inputs

Trigger a manual build with `cargo clean` enabled:

```bash
act workflow_dispatch -W .github/workflows/build.yaml -j build --input clean_cargo=true
```

Run the job in release-test mode:

```bash
act workflow_dispatch -W .github/workflows/build.yaml -j build --input release_test=true
```

### Release workflow

The release workflow uses `softprops/action-gh-release`, which needs a GitHub token to publish. By default, `act` does not have access to GitHub secrets, so the publish step is skipped unless you provide a token.

Run the release workflow without publishing (recommended for local verification):

```bash
act workflow_dispatch -W .github/workflows/build-release.yml -j build-release
```

Run with a token if you intentionally want to publish or update a real release:

```bash
act workflow_dispatch \
  -W .github/workflows/build-release.yml \
  -j build-release \
  -s GITHUB_TOKEN=ghp_xxx
```

> **Warning:** A real token will create or update a real GitHub release. Use this only when you intend to publish.

#### Keep release artifacts in your local checkout

By default, `act` runs inside a copied container workspace. Generated files, including release tarballs, can stay inside the container. Use `--bind` to write them back to your local checkout:

```bash
act workflow_dispatch \
  -W .github/workflows/build-release.yml \
  -j build-release \
  --bind
```

With `--bind`, artifacts are written to:

```text
target/release-artifacts/<version>/<version>/bin/
target/release-artifacts/<version>/<version>/lib/
target/release-artifacts/<version>/<version>/lib/lib-list.txt
target/release-artifacts/<version>/<version>/manifest.txt
target/release-artifacts/<version>/mududb-<version>-x86_64-unknown-linux-gnu.tar.gz
target/release-artifacts/<version>/mududb-<version>-x86_64-unknown-linux-gnu.tar.gz.sha256
target/release-artifacts/<version>/CHANGELOG_RELEASE.md
```

When the `act` job runs as root with `--bind`, the workflow resets ownership of the generated `target/release-artifacts/<version>` directory to the local checkout owner and group, and mirrors owner permissions to group/other users.

### Other workflows

Run the CI hardening checks:

```bash
act -W .github/workflows/ci-hardening.yaml
```

Run the nightly checks (udeps, fuzz, Miri, AddressSanitizer):

```bash
act -W .github/workflows/nightly-checks.yaml
```

#### Miri and isolation

The Miri job needs `MIRIFLAGS=-Zmiri-disable-isolation` because several tests call host APIs such as `clock_gettime`. The workflow already sets this environment variable in CI. When reproducing locally, run:

```bash
NIGHTLY_TOOLCHAIN="$(tr -d '[:space:]' < .rust-nightly-version)"
MIRIFLAGS="-Zmiri-disable-isolation" cargo +${NIGHTLY_TOOLCHAIN} miri test --workspace
```

Without `-Zmiri-disable-isolation`, Miri stops with an error like:

```text
unsupported operation: `clock_gettime` with `REALTIME` clocks not available when isolation is enabled
```

Run the compatibility workflow:

```bash
act -W .github/workflows/compatibility.yaml
```

### Optional: share local Rust caches

To reuse your host's Rust toolchain and crate cache, export the standard directories before running `act`:

```bash
export CARGO_HOME="$HOME/.cargo"
export RUSTUP_HOME="$HOME/.rustup"
act -W .github/workflows/build.yaml -j build
```

This avoids re-downloading dependencies inside the container on every run.

---

## Troubleshooting

### `io_uring_queue_init_params error -1`

This means the job container cannot initialize `io_uring`. Common causes:

1. The host kernel does not support `io_uring`, or it is disabled.

   ```bash
   uname -r
   cat /proc/sys/kernel/io_uring_disabled
   ```

2. The container was not started with `--privileged` and both `seccomp=unconfined` and `apparmor=unconfined`.

   Inspect the running container:

   ```bash
   docker ps
   docker inspect <container_id> --format '{{.HostConfig.Privileged}}'
   docker inspect <container_id> --format '{{json .HostConfig.SecurityOpt}}'
   ```

   Expected:

   - `Privileged` is `true`.
   - `SecurityOpt` contains both `seccomp=unconfined` and `apparmor=unconfined`.

3. A stale container was created with the wrong security options before `.actrc` was fixed.

   Remove the old container and rerun:

   ```bash
   docker rm -f <old_act_container_id>
   act -W .github/workflows/build.yaml -j build --pull=true
   ```

### `Parameter token or opts.auth is required` in the release workflow

The release job needs a `GITHUB_TOKEN` secret. Either:

- Omit the token if you only want to build and verify artifacts (the publish step is skipped).
- Pass a real token with `-s GITHUB_TOKEN=ghp_xxx` if you intend to publish.

### Workflow changes are not picked up

`act` caches action definitions and images aggressively. If you edit a workflow file and the old behavior persists:

```bash
# Remove cached act containers and images if needed
docker ps -a | grep act
docker rm -f <old_act_container_id>

# Re-pull the runner image
act -W .github/workflows/build.yaml -j build --pull=true
```

### Build is slow

- Use `--reuse` after confirming the container security settings are correct.
- Export `CARGO_HOME` and `RUSTUP_HOME` to reuse host caches.
- Set `CARGO_INCREMENTAL=0` is already done in the workflow; do not override it.

---

## Reference

- [`act` documentation](https://nektosact.com/introduction.html)
- [Docker installation guide](https://docs.docker.com/engine/install)
- Repository `.actrc` (used automatically):

  ```text
  --container-architecture linux/amd64
  -P ubuntu-latest=ghcr.io/catthehacker/ubuntu:act-latest
  --pull=false
  --action-offline-mode
  --privileged
  --container-options --security-opt=seccomp=unconfined
  --container-options --security-opt=apparmor=unconfined
  ```

---

## Summary of useful commands

| Goal | Command |
|------|---------|
| Run build workflow with log summary | `./script/ci/run-local.sh` |
| Run a specific workflow with summary | `./script/ci/run-local.sh .github/workflows/nightly-checks.yaml` |
| Run a single job | `./script/ci/run-local.sh -j cargo-deny` |
| Dry run | `act -W .github/workflows/build.yaml -j build --dryrun` |
| First run / refresh image | `act -W .github/workflows/build.yaml -j build --pull=true` |
| Normal build | `act -W .github/workflows/build.yaml -j build` |
| Verbose build | `act -W .github/workflows/build.yaml -j build -v` |
| Fast reuse | `act -W .github/workflows/build.yaml -j build --reuse` |
| With `cargo clean` | `act workflow_dispatch -W .github/workflows/build.yaml -j build --input clean_cargo=true` |
| Release (no publish) | `act workflow_dispatch -W .github/workflows/build-release.yml -j build-release --bind` |
| Release (publish) | `act workflow_dispatch -W .github/workflows/build-release.yml -j build-release -s GITHUB_TOKEN=ghp_xxx --bind` |
| Nightly checks | `act -W .github/workflows/nightly-checks.yaml` |
