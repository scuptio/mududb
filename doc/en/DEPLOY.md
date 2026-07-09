# MuduDB One-Click Deployment Guide

This guide walks you through installing dependencies, building, starting, and testing MuduDB on a clean Ubuntu 24.04 environment.

## Quick Start

```bash
# 1. Install system dependencies and Rust toolchains
bash script/shell/install_deps.sh

# 2. Build all components and package the example app
bash script/shell/build_all.sh

# 3. Start the server and run CRUD tests
bash script/shell/run_test.sh
```

If all three steps pass, the deployment is successful.

## Requirements

- Ubuntu 24.04 LTS (x86_64)
- Linux kernel 5.1+ with `CONFIG_IO_URING=y`
- 8GB+ RAM, 20GB+ disk
- Internet access (to download Rust toolchains and dependencies)

### Checking io_uring support

```bash
# Kernel version
uname -r  # must be >= 5.1

# Is io_uring available?
grep io_uring /proc/filesystems && echo "io_uring OK" || echo "NOT supported"

# memlock limit (io_uring needs locked memory)
ulimit -l  # recommended >= 65536 or unlimited
```

### Docker environment

When running inside Docker, use `--privileged` so the container can access the host kernel's io_uring:

```bash
docker run --privileged -v /path/to/repo:/mududb:ro ubuntu:24.04 ...
```

Or use finer-grained capabilities:

```bash
docker run --cap-add CAP_SYS_ADMIN --ulimit memlock=-1:-1 ...
```

## Script reference

### `script/shell/install_deps.sh`

Installs all dependencies required to run MuduDB:

| Category | Contents |
|----------|----------|
| System packages | python3, pip, python-is-python3, build-essential, curl, liburing-dev, clang, libclang-dev, llvm-dev, pkgconf, iproute2 |
| Rust | pinned stable toolchain (with rustfmt, clippy, x86_64-unknown-linux-gnu and wasm32-wasip2 targets) + auxiliary nightly toolchain |
| Python | toml, tomli-w |
| Tools | cargo-make |

> Implementation reference: `script/shell/install_deps.sh`, `rust-toolchain.toml`.

### `script/shell/build_all.sh`

Builds and installs all components:

1. `cargo build --release` — builds the workspace crates
2. `python3 script/build/install_binaries.py` — installs binaries to `~/.cargo/bin/` (mudud, mcli, mpm-build, mgen, mtp)
3. `cargo make` in `example/wallet` — regenerates entity code, transpiles procedures, builds the wallet example, and produces a `.mpk` package

The wallet `Makefile.toml` reinstalls the workspace CLI tools from source before generating code, so the package is always built with the current commit's toolchain.

### `script/shell/run_test.sh`

Starts the MuduDB server and runs CRUD tests:

- Creates a temporary data directory and writes a default config file
- Starts the `mudud` server (HTTP 8300 / TCP 9527)
- Installs the wallet example app
- Tests CREATE / READ / UPDATE / INVOKE / DELETE
- Cleans up automatically after the test

### `script/shell/debug_test.sh`

Integration test with extra diagnostics, outputting more detailed logs and intermediate state.

## Run everything in one go

```bash
#!/bin/bash
set -euo pipefail
bash script/shell/install_deps.sh
source "$HOME/.cargo/env"
bash script/shell/build_all.sh
bash script/shell/run_test.sh
bash script/shell/debug_test.sh
echo "All tests passed."
```
