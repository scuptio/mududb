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
bash script/ci/act.sh -W .github/workflows/build.yaml -j build
```

This command:

- Uses the settings in `.actrc`.
- Runs the `build` job from `.github/workflows/build.yaml`.
- Reuses the local Docker image by default (see [Why we need a custom `.actrc`](#why-we-need-a-custom-actrc)).

Both `.actrc` and the wrapper use `--bind=false`: act copies source files into
its container workspace instead of mounting your checkout. Builds write to the
container's `target/`, leaving your local `target/` untouched. Gitignored files,
including the host `target/`, are excluded from the copy by default.

The existing image and containers are reused (`--pull=false`, `--reuse=true`,
`--rm=false`). This preserves container build caches without building or pulling
a new runner image each time. Removing the container and its workspace storage
loses those caches. The wrapper captures console output in local `logs/`;
workflow-generated files stay inside the container.

Before starting any job, the wrapper checks that tracked and non-ignored
untracked source files are readable and their directories accessible. It also
checks Docker for retained act containers whose bind mounts overlap this checkout.
Failures stop the run early and list the affected paths or containers; the
checks never change permissions or remove containers. Python 3, Git, and access
to the Docker daemon are required for these checks.

When upgrading from the old bind configuration, first follow
[Switch from bound containers](#switch-from-bound-containers). Existing containers
retain their original mounts. The wrapper rejects bind-enabling arguments and
forces `--bind=false`. Direct `act` commands bypass these preflight checks;
use the wrapper for normal runs.

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
--bind=false
--concurrent-jobs=1
--reuse=true
--rm=false
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
| `--bind=false` | Copy source into an isolated container workspace; leave host build files untouched. |
| `--reuse=true`, `--rm=false` | Retain containers and their build state between runs. |
| `--concurrent-jobs=1` | Run one job at a time to limit resource usage. |
| `--env ACT=true` | Expose `env.ACT` so workflows can skip CI-only steps (e.g. disk cleanup) during local runs. |

> **Important:** The `--privileged` and `--security-opt` flags are required for the `io_uring` tests. Without them, you will see errors such as `io_uring_queue_init_params error -1`.

Direct act command-line flags can override `.actrc` settings. The wrapper keeps
`--bind=false` as the final argument and rejects attempts to enable bind mode.

---

## Running workflows

### Helper script

The repository provides `script/ci/act.sh`, which wraps `act` and captures logs for each workflow or job in a single place. It is the easiest way to run actions locally without losing errors in long output.

```bash
# Run the main build workflow and capture a summary
./script/ci/act.sh

# Run a specific workflow
./script/ci/act.sh .github/workflows/nightly-checks.yaml

# Run one or more jobs individually
./script/ci/act.sh -j cargo-deny
./script/ci/act.sh -j build -j rustdoc

# Show help
./script/ci/act.sh --help
```

For each run, the script writes a log file under `logs/` and prints a final summary. If anything failed, it extracts the last matching error lines so you do not have to scroll through the full `act` output.

### Log files from workflow steps

The CI workflows also write per-step log files inside the container workspace:

- `logs/build-*.log` — `build.yaml` steps (check, clippy, test, etc.)
- `logs/hardening-*.log` — `ci-hardening.yaml` steps (fmt, deny, hack, outdated)
- `logs/nightly-*.log` — `nightly-checks.yaml` steps (udeps, fuzz, asan, coverage) and `miri.yaml` steps (`nightly-miri.log`)
- `logs/compat-*.log` — `compatibility.yaml` steps

Steps use `set -o pipefail` with `tee` to preserve failures. These files remain in the retained container workspace; use the [export procedure](#export-container-artifacts-and-logs) to copy them to the host. The wrapper’s local console logs remain available without exporting.

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

The repository `.actrc` retains containers between runs (`--reuse=true` and
`--rm=false`). This avoids Docker container-removal timeouts during local job
cleanup, but preserves container state and consumes disk space between runs.
It does not repair an unresponsive Docker daemon.

To request fresh containers and cleanup after failures for a run:

```bash
act -W .github/workflows/build.yaml -j build --reuse=false --rm=true
```

### Build workflow inputs

`cargo clean` is disabled by default, including for push and pull-request runs.
Enable it explicitly for a manual build:

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
act workflow_dispatch -W .github/workflows/build-release.yaml -j build-release
```

Run with a token if you intentionally want to publish or update a real release:

```bash
act workflow_dispatch \
  -W .github/workflows/build-release.yaml \
  -j build-release \
  -s GITHUB_TOKEN=ghp_xxx
```

> **Warning:** A real token will create or update a real GitHub release. Use this only when you intend to publish.

#### Export container artifacts and logs

Release output remains under `target/release-artifacts/<version>/` in the
container workspace. Export only the files you need after the job completes.
Identify the container for the desired job and inspect its workspace path:

```bash
docker ps -a --filter name=act- --format '{{.ID}} {{.Names}} {{.Status}}'
docker inspect <container_id> --format '{{.Config.WorkingDir}}'
```

Substitute that workspace path below. Run these commands as your normal host
user, without `sudo` or `docker cp -a`, so copied files belong to your host user:

```bash
mkdir -p logs/act-export/release-artifacts logs/act-export/steps
docker cp <container_id>:<workspace_path>/target/release-artifacts/. logs/act-export/release-artifacts/
docker cp <container_id>:<workspace_path>/logs/. logs/act-export/steps/
```

The exported release directory contains the binaries, libraries, manifest,
archive, checksum, and changelog. This export does not populate the host's Cargo
build cache.

### Other workflows

Run the CI hardening checks:

```bash
act -W .github/workflows/ci-hardening.yaml
```

Run the nightly checks (udeps, fuzz, AddressSanitizer):

```bash
act -W .github/workflows/nightly-checks.yaml
```

Run the Miri checks (manual-only workflow; it is not triggered by push/PR on GitHub):

```bash
act workflow_dispatch -W .github/workflows/miri.yaml
```

> **Warning:** the full-workspace Miri run takes hours under the interpreter and
> some stress-scale tests (e.g. the 3000-record concurrent time-series test)
> are effectively infeasible under Miri. Expect a long run.

#### Miri and isolation

The Miri job needs `MIRIFLAGS=-Zmiri-disable-isolation` because several tests call host APIs such as `clock_gettime`. The `miri.yaml` workflow already sets this environment variable. When reproducing locally, run:

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

### Build cache reuse

Keep `--reuse=true` and `--rm=false`, and leave `clean_cargo` disabled for normal
runs. Each retained job workspace keeps its own `target/`; it does not share the
host's Cargo target directory. Cargo registry caching is also configured by the
workflows. Exporting host `CARGO_HOME` or `RUSTUP_HOME` alone does not mount those
directories into act containers.

---

## Troubleshooting

### Unreadable source files during checkout

Copy mode reads source files as the host user. Files left by earlier root bind
builds can therefore fail checkout even when the container runs as root. The
wrapper reports these before starting any workflow and never prints contents.
Gitignored build caches are excluded from this source check.

Inspect the listed paths in a host terminal. If a file belongs to the wrong
user, restore only that file's owner to the checkout owner, for example:

```bash
stat -c '%a %U:%G %n' example/mpk/wallet-cs/nuget.config
sudo chown --reference=. -- example/mpk/wallet-cs/nuget.config
test -r example/mpk/wallet-cs/nuget.config
```

This preserves its permission bits, including `600`; there is no need to make
the file readable by everyone. Directories also require read and search access.
If ownership is already correct, inspect the reported directory modes or ACLs
instead. Rerun the wrapper after repairing the reported paths.

### Switch from bound containers

Stop any act run before migrating. List containers and inspect their mounts:

```bash
docker ps -a --filter name=act- --format '{{.ID}} {{.Names}} {{.Status}}'
docker inspect <container_id> --format '{{json .Mounts}}'
```

For each old container whose bind mount points to this checkout, export any
needed artifacts first, then remove that specific container. Do not remove
unrelated containers, images, or volumes:

```bash
docker stop <old_act_container_id>  # only if it is still running
docker rm <old_act_container_id>
```

The next run creates a container with the isolated workspace using the existing
image; no image rebuild or pull is required. Inspect its mounts to confirm the
checkout is no longer bound. Docker socket mounts are separate from workspace
mounts.

If earlier runs left the host `target/` owned by root, repair it once from the
repository root in a host terminal, after all builds have stopped:

```bash
sudo chown -R -- "$(id -u):$(id -g)" target
```

Neither container removal nor this ownership repair is performed automatically.

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
- Leave `clean_cargo` disabled to retain the container workspace’s build cache.
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
  --env ACT=true
  --bind=false
  --concurrent-jobs=1
  --reuse=true
  --rm=false
  ```

---

## Summary of useful commands

| Goal | Command |
|------|---------|
| Run build workflow with log summary | `./script/ci/act.sh` |
| Run a specific workflow with summary | `./script/ci/act.sh .github/workflows/nightly-checks.yaml` |
| Run a single job | `./script/ci/act.sh -j cargo-deny` |
| Dry run | `act -W .github/workflows/build.yaml -j build --dryrun` |
| First run / refresh image | `act -W .github/workflows/build.yaml -j build --pull=true` |
| Normal build | `act -W .github/workflows/build.yaml -j build` |
| Verbose build | `act -W .github/workflows/build.yaml -j build -v` |
| Fast reuse | `act -W .github/workflows/build.yaml -j build --reuse` |
| With `cargo clean` | `act workflow_dispatch -W .github/workflows/build.yaml -j build --input clean_cargo=true` |
| Release (no publish) | `act workflow_dispatch -W .github/workflows/build-release.yaml -j build-release` |
| Release (publish) | `act workflow_dispatch -W .github/workflows/build-release.yaml -j build-release -s GITHUB_TOKEN=ghp_xxx` |
| Nightly checks | `act -W .github/workflows/nightly-checks.yaml` |
| Miri (manual-only) | `act workflow_dispatch -W .github/workflows/miri.yaml` |
