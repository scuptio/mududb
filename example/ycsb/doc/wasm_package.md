# YCSB Wasm Package Build

This target is for building the YCSB example as a wasm/package artifact instead of running the local benchmark runner.

## 1. Prerequisites

Tools:

- `rustup`
- `cargo`
- `cargo-make`
- `python3`
- `mtp`
- `mpk`

Rust target:

```bash
rustup target add wasm32-wasip2
```

If `cargo-make` is missing:

```bash
cargo install cargo-make
```

If the transpiler script dependencies are missing:

```bash
python3 -m venv .venv
. .venv/bin/activate
pip install tomli-w toml
```

## 2. Build

From the example directory:

```bash
cd example/ycsb
cargo make
```

This runs:

1. transpile `src/rust` into async code under `src/generated`
2. build the wasm artifact for `wasm32-wasip2`
3. package the output into `.mpk`

## 3. Output

Expected output artifact:

```bash
target/wasm32-wasip2/release/ycsb.mpk
```

The wasm file is built at:

```bash
target/wasm32-wasip2/release/ycsb.wasm
```

## 4. Notes

This build path does not use the standalone adapter.
It is the packaging path for wasm deployment/testing.

The SQL files are intentionally empty for this example because the YCSB workload uses only the key/value syscall surface.
