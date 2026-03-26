# How to Start

## Clone the Repository

```bash
git clone https://github.com/scuptio/mududb.git
```
## Prerequisite Setup(Ubuntu or Debian)

### System packages

Install the native build dependencies first:

```bash
sudo apt-get update -y
sudo apt-get install -y python3 python3-pip build-essential curl liburing-dev
```

These packages are used for:

- `python3` and `python3-pip`: required by the example build scripts
- `build-essential`: required for native compilation on Linux
- `curl`: used to install Rust via `rustup`
- `liburing-dev`: required only for the Linux native `io_uring` backend used by `mudu_kernel`

If you are building on Windows, you do not need `liburing-dev`, because the native `io_uring` path is Linux-only.

### Rust toolchain

Use the nightly Rust toolchain:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup toolchain install nightly
rustup default nightly
rustup component add rustfmt --toolchain nightly
rustup update nightly
rustup target add wasm32-wasip2
```
### Python packages

The example build scripts use Python packages at runtime:

```bash
python -m pip install toml tomli-w
```

### cargo make

The example applications are driven by `cargo-make` task files, so installing it is recommended:

```bash
cargo install cargo-make
```

## Install Tools and MuduDB Server

```bash
python script/build/install_binaries.py
```

By default, this installs the supported release tools:

- `mpk`: package builder
- `mgen`: source generator
- `mtp`: transpiler
- `mudud`: MuduDB server
- `mcli`: TCP protocol client CLI

If you need to install every workspace binary target instead, use:

```bash
python script/build/install_binaries.py --all-workspace-bins
```


## Create a Configuration File 

[mududb_cfg.toml example](../cfg/mududb_cfg.toml)

Create the configuration file at:

```bash
touch ${HOME}/.mudu/mududb_cfg.toml
```

## Use MuduDB

### 1. Start `mudud`

```bash
mudud
```

After `mudud` is running, you can verify the built-in key/value access first, then build and install an `.mpk` example package.

### 2. Use mcli to put/get a key

Each `mcli` command creates and closes its own temporary session automatically, so you do not need to pass a `session_id`.

```bash
mcli put --json '{
  "key": "user-1",
  "value": "value-1"
}'

mcli get --json '{
  "key": "user-1"
}'
```

The `get` command should return:

```json
"value-1"
```

### 3. Build, install, and use a MuduDB application

#### Build the key-value `.mpk` package

```bash
cd example/key-value
cargo make
```

The package target is generated at:

```bash
target/wasm32-wasip2/release/key-value.mpk
```

#### Install the `.mpk` package with mcli

```bash
mcli app-install --mpk target/wasm32-wasip2/release/key-value.mpk
```

#### Invoke procedures from the installed `.mpk` package

Insert a record through the `kv_insert` procedure:

```bash
mcli app-invoke --app kv --module key_value --proc kv_insert --json '{
  "user_key": "user-1",
  "value": "value-from-mpk"
}'
```

Read it back through the `kv_read` procedure:

```bash
mcli app-invoke --app kv --module key_value --proc kv_read --json '{
  "user_key": "user-1"
}'
```
