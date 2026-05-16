# How to Start

## Quick Install with `mudup`

This path is for server deployment and daily usage. It does not require building MuduDB from source.

### 1. Install `mudup`

Install `mudup` from the release artifact, then verify:

```bash
curl --proto '=https' --tlsv1.2 -fsSL  https://github.com/scuptio/mudup/releases/download/latest/mudup-init.sh | sh
mudup --help
```

### 2. Install MuduDB and its toolchain

```bash
mudup install
```

This installs and activates the latest release (`mudud`, `mcli`, `mpk`, `mgen`, `mtp`).

### 3. Verify installation

```bash
mudud --version
mcli --version
```

`mudup install` also creates `${HOME}/.mududb/mududb_cfg.toml` with default values if it does not already exist.

## Source Build (For Development)

## Clone the Repository

```bash
git clone https://github.com/scuptio/mududb.git
```
## Prerequisite Setup (Ubuntu or Debian)

### System packages

Install the native build dependencies first:

```bash
sudo apt-get update -y
sudo apt-get install -y \
    python3 \
    python3-pip \
    python-is-python3 \
    build-essential \
    curl liburing-dev \
    clang \
    libclang-dev \
    llvm-dev \
    pkgconf
```

These packages are used for:

- `python3` and `python3-pip`: required by the example build scripts
- `build-essential`: required for native compilation on Linux
- `curl`: used to install Rust via `rustup`
- `liburing-dev`: required only for the Linux native `io_uring` backend used by `mudu_kernel`
- `clang`, `libclang-dev`, `llvm-dev`: required by *[bindgen](https://github.com/rust-lang/rust-bindgen)*


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

## Install Tools and MuduDB Server (Development Build)

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
mkdir -p ${HOME}/.mududb
touch ${HOME}/.mududb/mududb_cfg.toml
```

If the file does not exist, `mudud` also creates `${HOME}/.mududb/mududb_cfg.toml` automatically on first start with default values.

## Use MuduDB

Optional reading: [`mcli` Management Interface (HTTP)](./mcli_admin.md).

### 1. Start `mudud`

Before starting `mudud`, make sure the server process has a sufficiently high open-files limit. A low soft `nofile` limit such as `1024` can cause stalls or failed session setup under higher connection counts, even if your interactive shell is configured differently.

For a shell-launched local server, you can raise it before starting `mudud`:

```bash
ulimit -n 65535
mudud
```

If `mudud` is launched by `systemd` or another supervisor, configure the service-level file descriptor limit there as well, for example `LimitNOFILE=65535`.

You can verify the live limit after startup with:

```bash
cat /proc/$(pgrep -x mudud)/limits | rg 'open files'
```

After `mudud` is running, you can first use interactive SQL CRUD, then install and run the `wallet` example app.

### 2. Use mcli interactive shell for CRUD

Start the interactive shell:

```bash
mcli --addr 127.0.0.1:9527 shell --app demo
```

Run a complete CRUD flow in the shell:

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

#### Build the wallet `.mpk` package

```bash
cd example/wallet
cargo make
```

The package target is generated at:

```bash
target/wasm32-wasip2/release/wallet.mpk
```

#### Install wallet with mcli

```bash
mcli --http-addr 127.0.0.1:8300 app-install --mpk target/wasm32-wasip2/release/wallet.mpk
```

After installation, verify with the HTTP management commands:

```bash
mcli --http-addr 127.0.0.1:8300 app-list
mcli --http-addr 127.0.0.1:8300 app-detail --app wallet
mcli --http-addr 127.0.0.1:8300 app-detail --app wallet --module wallet --proc create_user
mcli --http-addr 127.0.0.1:8300 server-topology
```

#### Invoke wallet procedures

Create two users:

```bash
mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke --app wallet --module wallet --proc create_user --json '{
  "user_id": 1001,
  "name": "Alice",
  "email": "alice@example.com"
}'

mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke --app wallet --module wallet --proc create_user --json '{
  "user_id": 1002,
  "name": "Bob",
  "email": "bob@example.com"
}'
```

Note: `app-invoke` sends the procedure call over TCP; it still needs `--http-addr` to fetch procedure metadata.

Deposit and transfer:

```bash
mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke --app wallet --module wallet --proc deposit --json '{
  "user_id": 1001,
  "amount": 5000
}'

mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke --app wallet --module wallet --proc transfer --json '{
  "from_user_id": 1001,
  "to_user_id": 1002,
  "amount": 1200
}'
```

Check wallet balances in shell:

```bash
mcli --addr 127.0.0.1:9527 shell --app wallet
```

```sql
SELECT user_id, balance FROM wallets WHERE user_id IN (1001, 1002);
```
