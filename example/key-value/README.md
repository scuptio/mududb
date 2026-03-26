# kv Example

This example implements a small synchronous kv-style workload on top of the key/value syscall API.

Procedures:

- `kv_insert`
- `kv_read`
- `kv_update`
- `kv_scan`
- `kv_read_modify_write`

All procedures use synchronous `mudu_get`, `mudu_put`, and `mudu_range`.
The session is provided by the procedure caller through the first procedure argument, so the procedures do not call `mudu_open` or `mudu_close`.

## Build dependencies

To build the `.mpk` package, make sure the following tools are installed:

- `rustup`
- `cargo`
- `cargo-make`
- `python3`
- `mtp`
- `mpk`

Python packages required by `script/build/transpiler.py`:

- `tomli-w`

Rust target required:

```bash
rustup target add wasm32-wasip2
```

If `cargo-make` is not installed:

```bash
cargo install cargo-make
```

If `tomli-w` is not installed and your system Python is externally managed, use one of these options.

Install from the OS package manager when available:



```bash
python3 -m venv .venv
. .venv/bin/activate
pip install tomli-w toml
```

`mtp` and `mpk` must also be available in `PATH`. In this workspace they are expected to come from the project build/install flow.

## Build `.mpk`

```bash
cd example/key-value
cargo make
```

The package target is generated at `target/wasm32-wasip2/release/key-value.mpk`.
