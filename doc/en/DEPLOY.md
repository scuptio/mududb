# Local build and verification on Ubuntu

This page describes the repository's build and test scripts. `run_test.sh` starts a temporary server and cleans up after the test. These scripts do not install a persistent service or cover production access control, backups and upgrades.

For a first run, follow [environment setup](how_to_start.md), then [Your first MPK](your_first_mpk.md) to start and invoke the example manually. Run the scripts below from the repository root and check each script's paths and prerequisites first.

## Requirements

- Ubuntu 24.04 LTS (x86_64)
- A Linux kernel and security policy compatible with the selected I/O mode
- 8GB+ RAM, 20GB+ disk
- Internet access (to download Rust toolchains and dependencies)

### Inspect the runtime environment

From the repository root:

```bash
bash script/build/inspect_iouring_env.sh
```

The script reports kernel toggles, seccomp status and container settings for diagnosis; it is not a complete functional test. `io_uring` is not a filesystem and cannot be detected through `/proc/filesystems`.

Containers also depend on the host kernel and security policy. See [local act setup](../../.github/run-actions-locally.md) for the CI container options and [Running mudud](running_mudud.md) for persistent server configuration.

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

1. `cargo build --release` in each of the four group workspaces under `crates/` (`common`, `db-kernel`, `sdk`, `tools`) — builds all crates
2. `python3 script/build/install_binaries.py` — installs binaries to `~/.cargo/bin/` (mpm-build, mpm-crate, mpm-install, mgen, mtp, mudud, mcli)
3. `cargo make` in `example/mpk/wallet` — regenerates entity code, transpiles procedures, builds the wallet example, and produces a `.mpk` package

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
