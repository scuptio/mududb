# TPC-C Cross-Database Benchmark

`bench_cross_db.py` is a structured, extensible Python orchestrator that runs the
TPC-C benchmark against six database configurations and compares throughput
(TPS) and P99 latency under different CPU core limits and client connection
counts.

## Table of contents

- [What is measured](#what-is-measured)
- [Prerequisites](#prerequisites)
- [Installing prerequisites](#installing-prerequisites)
  - [Rust toolchain](#rust-toolchain)
  - [cargo-make](#cargo-make)
  - [PostgreSQL](#postgresql)
  - [MySQL](#mysql)
  - [Python packages](#python-packages)
- [Building the project](#building-the-project)
- [Running the benchmark](#running-the-benchmark)
  - [Generate a configuration file](#generate-a-configuration-file)
  - [Edit the configuration](#edit-the-configuration)
  - [Run](#run)
  - [Run only a subset of backends](#run-only-a-subset-of-backends)
  - [Override the output directory](#override-the-output-directory)
- [Two-machine (remote) mode](#two-machine-remote-mode)
- [Configuration reference](#configuration-reference)
- [CPU core limiting](#cpu-core-limiting)
- [Outputs](#outputs)
- [Troubleshooting](#troubleshooting)
- [Extending the script](#extending-the-script)

## What is measured

| Backend | Benchmark mode | How the Rust runner connects |
|---|---|---|
| PostgreSQL | `interactive` | `MUDU_CONNECTION=postgres://...` via `mudu_adapter` |
| PostgreSQL | `pg-procedure` | Same connection; the client installs PL/pgSQL procedures (`sql/procedures_postgres.sql`) and runs each transaction as one `SELECT tpcc_*()` call; backend key `postgres-procedure` |
| MySQL | `interactive` | `MUDU_CONNECTION=mysql://...` via `mudu_adapter` |
| MuduDB | `interactive` | `MUDU_CONNECTION=mudud://<tcp>?http_addr=<http>` |
| MuduDB | `stored-procedure` | `--mode stored-procedure --tcp-addr ... --http-addr ... --mpk ...` |
| MuduDB (io_uring) | `stored-procedure` | Same as above; backend key `mududb-procedure-iouring` |
| SpacetimeDB | `stored-procedure` (reducer) | Standalone client `tpcc-stdb-benchmark` (calls reducers via `spacetimedb-sdk`) |

The SpacetimeDB backend is **opt-in**: it is not part of the default
`--backends` list. Pass `--backends spacetimedb` (or add it to a custom
list) to include it in a run.

Report outputs (summary table, `summary.csv`, chart legends) use short
display labels instead of the raw `backend/mode` keys; the raw keys in
`results.json` are unchanged so result files stay mergeable:

| Display label | Raw key | Meaning |
|---|---|---|
| `postgres-i` | `postgres/sync` | PostgreSQL **interactive** via `mudu_adapter` |
| `postgres-n` | `postgres-procedure/pg-procedure` | PostgreSQL **near-data-processing**: PL/pgSQL stored procedures executed inside the server |
| `mysql` | `mysql/sync` | MySQL via `mudu_adapter` |
| `mududb-i` | `mududb-interactive-iouring/sync` | MuduDB **interactive** client over the sync adapter |
| `mududb-n` | `mududb-procedure-iouring/tcp-multi-port` | MuduDB **near-data-processing**: stored procedures executed next to the data |
| `spacetimedb` | `spacetimedb/spacetimedb-reducer` | SpacetimeDB reducer mode |

Each MuduDB row is one entry in the `mudud.modes` config mapping: the entry
name becomes the backend key `mududb-<name>`, and its sub-options pick the
server I/O mode (`server_mode: tokio | iouring`) and the client mode
(`interactive_mode: interactive | procedure`). The three rows above are the
default entries; entry names are free-form, so e.g. adding a `procedure-tokio`
entry benchmarks the procedure client against a tokio server in the same run.

For the MuduDB configurations:

- `tcp_multi_port = true` is enabled (port sharding).
- `worker_threads` is set to the current CPU core limit.
- Warehouse partitioning is **enabled**: the benchmark creates a partition rule
  with one range partition per worker by default (each covering a contiguous
  warehouse id range; the count is tunable via `mudud.partition_count`) and a
  placement mapping each partition to its worker, plus the partitioned schema,
  so each worker owns the data of its warehouses instead of funnelling all
  data access into worker 0. Procedure mode installs
  `tpcc_partitioned.mpk` (which ships no DDL) and initializes the schema and
  seed data through the statement-routed sync adapter.

PostgreSQL and MySQL use the same Rust `tpcc-benchmark` binary in interactive
mode; only the `MUDU_CONNECTION` environment variable changes. The
`postgres-procedure` backend uses the same binary with `--mode pg-procedure`:
during schema initialization the client installs the PL/pgSQL procedures from
`sql/procedures_postgres.sql` into the fresh database, then runs every
transaction as a single `SELECT tpcc_*()` call (auto-committed, so each call
is its own transaction and a procedure exception counts as an abort).

## Cross-backend alignment

The benchmark deliberately aligns the three database families on the
following points:

- **Isolation level**: PostgreSQL is started with
  `default_transaction_isolation='read committed'` so that concurrent
  same-row UPDATEs queue on row locks. MySQL is started with
  `--transaction-isolation=REPEATABLE-READ` (InnoDB's default, set explicitly),
  where concurrent same-row UPDATEs also queue on locks. MuduDB natively
  implements snapshot isolation and resolves write-write conflicts without
  aborting either transaction. PostgreSQL's REPEATABLE READ was tried first
  but is snapshot isolation with first-committer-wins semantics: every
  concurrent update of the same row fails with 40001 "could not serialize
  access due to concurrent update", which inflated the TPC-C abort rate to
  50-84% on the district hot row while the other backends reported ~0%.
  READ COMMITTED is PostgreSQL's default and the standard choice for TPC-C
  on PostgreSQL (e.g. HammerDB).
- **Buffer pool memory**: PostgreSQL `shared_buffers` and MySQL
  `innodb_buffer_pool_size` are both set to `buffer_pool_ratio` (default 0.8,
  i.e. 80%) of total system RAM read from `/proc/meminfo`. MuduDB has no
  buffer pool configuration — its page cache is an unbounded in-process hash
  map backed by the OS page cache — so no equivalent setting exists.
- **Money column types**: all money-amount columns (`w_ytd`, `d_ytd`,
  `c_balance`, `c_ytd_payment`, `i_price`, `ol_amount`, `h_amount`) are
  declared as `NUMERIC(p,s)` in the DDL of all three backends instead of
  `FLOAT`/`DOUBLE`. Rate columns (`w_tax`, `d_tax`, `c_discount`) are ratios,
  not money amounts, and stay `INTEGER`. Values remain whole numbers
  (e.g. `42.00`) to keep the workload semantics unchanged.

## Prerequisites

- A Linux machine with `taskset` installed (standard in `util-linux`).
- Rust toolchain: `rustup`, `cargo`, and the `wasm32-wasip2` target.
- `cargo-make` for building `tpcc.mpk`.
- PostgreSQL server binaries: `initdb`, `pg_ctl`, `psql`.
- MySQL server binaries: `mysqld`, `mysqladmin`, `mysql`.
- SpacetimeDB CLI: auto-installed by the script (the pinned release tarball
  from GitHub is downloaded into `spacetimedb.install_dir`; pin an existing
  installation with `spacetimedb.cli_path` if you already have one).
  Only needed when the opt-in `spacetimedb` backend is enabled.
- The `wasm32-unknown-unknown` Rust target for the SpacetimeDB module (the
  script runs `rustup target add` automatically if it is missing).
- Python 3 with `PyYAML`. `matplotlib` is optional; charts are skipped if it is
  not installed.
- `install_prerequisites.sh` (optional helper) automates the installations
  above on Ubuntu/Debian and RHEL-family distributions.

## Installing prerequisites

### Automated install script

The fastest way is to run the provided helper script:

```bash
cd example/tpcc
chmod +x install_prerequisites.sh
./install_prerequisites.sh
```

It will ask for confirmation before each step. To run non-interactively (useful
for CI or fresh VMs):

```bash
./install_prerequisites.sh --yes
```

The script supports Ubuntu/Debian and RHEL/CentOS/Rocky/AlmaLinux/Fedora.

Notes:

- It detects and repairs broken/incomplete packages before installing new ones.
- It skips PostgreSQL or MySQL package installation if the required binaries
  (`initdb`/`pg_ctl`/`psql` or `mysqld`/`mysqladmin`/`mysql`) are already in
  `PATH`.
- If your system uses MySQL community edition instead of Ubuntu's `mysql-server`,
  set the package name explicitly:

  ```bash
  MYSQL_PKG=mysql-community-server ./install_prerequisites.sh --yes
  ```

### Manual install

If you prefer to install manually, follow the steps below.

#### Rust toolchain

If Rust is not installed yet:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

Add the WebAssembly target required by MuduDB packages:

```bash
rustup target add wasm32-wasip2
```

Verify:

```bash
rustc --version
cargo --version
rustup target list --installed | grep wasm32-wasip2
```

#### cargo-make

```bash
cargo install cargo-make
```

Verify:

```bash
cargo make --version
```

#### PostgreSQL

The script needs the server-side binaries, not just the client. On Ubuntu/Debian:

```bash
sudo apt-get update
sudo apt-get install -y postgresql postgresql-client
```

On RHEL/CentOS/Rocky:

```bash
sudo dnf install -y postgresql-server postgresql-contrib
```

Verify:

```bash
which initdb pg_ctl psql
```

#### MySQL

On Ubuntu/Debian:

```bash
sudo apt-get update
sudo apt-get install -y mysql-server
```

On RHEL/CentOS/Rocky:

```bash
sudo dnf install -y mysql-server
```

Verify:

```bash
which mysqld mysqladmin mysql
```

> **Note for MySQL:** the default `sql/ddl.sql` uses `h_id TEXT PRIMARY KEY`,
> which InnoDB rejects without an explicit prefix length. Use
> `sql/ddl_mysql.sql` in the benchmark configuration instead.

#### Python packages

```bash
python3 -m pip install --user pyyaml matplotlib
```

Or with a virtual environment (recommended):

```bash
python3 -m venv .venv
source .venv/bin/activate
python3 -m pip install pyyaml matplotlib
```

Verify:

```bash
python3 -c "import yaml, matplotlib; print('ok')"
```

## Building the project

The benchmark script uses `cargo run` internally, so a manual build is not
required. However, building once before the first run saves time during the
sweep:

```bash
# From the workspace root
cd /path/to/mududb_p
cargo build -p mudud --release
cargo build -p tpcc --features benchmark-runner --bin tpcc-benchmark
cd example/tpcc
cargo make package
```

This produces:

- `target/release/mudud`
- `target/debug/tpcc-benchmark`
- `target/wasm32-wasip2/release/tpcc.mpk`

If any of these are missing, the script will build them on demand.

## Running the benchmark

### Preset configurations

The repository ships three ready-to-use presets:

- `bench_cross_db_config.yaml`: the default template, MuduDB-only backend
  list; a starting point for custom configurations.
- `bench_showcase.yaml`: full 12-core comparison of PostgreSQL, MySQL, MuduDB
  (interactive + procedure over io_uring) and SpacetimeDB across the
  connection curve — highlights MuduDB's throughput/latency advantage at full
  core count.
- `bench_scalability.yaml`: CPU-core sweep (1 → 12) across three connection
  tiers with the same five backends — highlights how MuduDB scales with cores.
  Same dataset as the showcase preset, so results are comparable.

```bash
python3 bench_cross_db.py --config bench_showcase.yaml
python3 bench_cross_db.py --config bench_scalability.yaml
```

### Generate a configuration file

```bash
cd example/tpcc
python3 bench_cross_db.py --write-default-config bench_cross_db_config.yaml
```

This writes a YAML file with sensible defaults based on the number of physical
CPU cores detected on the machine.

### Edit the configuration

Configuration files use a two-level structure: `defaults` holds the global
settings, and `tests` is a list of named test sections. Each test inherits
everything from `defaults`; keys listed in a test replace the defaults for
that test only. Sweepable per-test keys include `backends`, `workload`,
`cpu_cores`, `connections`, `seckill_hotspot_percents`, dataset and timing
knobs; infrastructure sections (`postgres`, `mysql`, `mudud`, `spacetimedb`,
`remote`, `output_dir`, ...) are global-only and must stay in `defaults`.

Open `bench_cross_db_config.yaml` and adjust at least:

- `tests[*].cpu_cores` / `tests[*].connections`: which core limits and
  connection counts each test sweeps.
- `defaults.warehouses`, `defaults.operations`, etc.: TPC-C scale.
- `defaults.postgres.initdb`, `defaults.postgres.pg_ctl`,
  `defaults.postgres.psql`: paths if they are not in `PATH`.
- `defaults.mysql.mysqld`, `defaults.mysql.mysqladmin`, `defaults.mysql.mysql`:
  paths if they are not in `PATH`.
- `defaults.mysql.ddl`: set to `./sql/ddl_mysql.sql` for MySQL.
- `defaults.mudud.modes`: mapping of mode name to its `{server_mode,
  interactive_mode}` sub-options (see below).

Example for a machine with 8 physical cores:

```yaml
defaults:
  repeats: 3
  warehouses: 10
  operations: 5000

  postgres:
    initdb: initdb
    pg_ctl: pg_ctl
    psql: psql
    user: postgres
    password: postgres
    database: tpcc
    ddl: ./sql/ddl.sql

  mysql:
    mysqld: mysqld
    mysqladmin: mysqladmin
    mysql: mysql
    user: root
    password: ""
    database: tpcc
    ddl: ./sql/ddl_mysql.sql

  mudud:
    listen_ip: 127.0.0.1
    ring_entries: 1024
    modes:
      interactive:
        server_mode: iouring
        interactive_mode: interactive
      procedure-iouring:
        server_mode: iouring
        interactive_mode: procedure

  output_dir: ./bench_cross_db_results
  keep_data: false

tests:
  # CPU-core sweep at a fixed connection count.
  - name: cpu-scaling
    cpu_cores: [1, 2, 4, 8]
    connections: [64]

  # Connection sweep at full core count.
  - name: connection-scaling
    cpu_cores: [8]
    connections: [1, 2, 4, 8, 16, 32, 64]
```

Each test produces its own results (`results.json`, `summary.csv`, charts)
under `output_dir/<name>/`. Use `--dry-run` to print the expanded run matrix
per test without starting anything:

```bash
python3 bench_cross_db.py --config bench_cross_db_config.yaml --dry-run
```

### Run

```bash
python3 bench_cross_db.py --config bench_cross_db_config.yaml
```

The script will, for each test section, each backend and each
`(cores, connections, seckill_hotspot_percent, think_time_ms)` tuple:

1. Start the database server pinned to the selected cores with `taskset`.
2. Wait for the server to be ready.
3. Run `tpcc-benchmark` (pinned to client cores if configured).
4. Parse the printed summary.
5. Stop the server and clean up temporary data directories.

Progress is printed to the terminal. At the end of each test section a
summary table and PNG charts (TPS/P99 vs connections, vs cores, and vs
hotspot percent, and vs think time) are produced under `output_dir/<test-name>/`; charts whose
x-dimension has a single value are skipped.

### Run only a subset of backends

Backends can be selected by DB type in the config file — under `defaults`
(inherited by every test) or inside a single test:

```yaml
# postgres, postgres-procedure, mysql, mududb, spacetimedb;
# 'mududb' covers the MuduDB modes
defaults:
  backends: [postgres, mysql]
```

The MuduDB modes run for `mududb` are themselves configurable. `mudud.modes`
is a mapping from a free-form mode name to its sub-options — `server_mode`
(`iouring` (default) or `tokio`) and `interactive_mode` (`interactive` or
`procedure`, required). Each entry becomes a concrete backend key
`mududb-<name>`:

```yaml
mudud:
  modes:
    interactive:
      server_mode: tokio
      interactive_mode: interactive
    procedure-iouring:
      server_mode: iouring
      interactive_mode: procedure
```

or on the command line (which overrides the config list):

```bash
# Only MuduDB modes
python3 bench_cross_db.py --config bench_cross_db_config.yaml \
  --backends mududb

# Only the io_uring procedure variant
python3 bench_cross_db.py --config bench_cross_db_config.yaml \
  --backends mududb-procedure-iouring

# Only PostgreSQL and MySQL
python3 bench_cross_db.py --config bench_cross_db_config.yaml \
  --backends postgres,mysql

# Only SpacetimeDB (reducer mode)
python3 bench_cross_db.py --config bench_cross_db_config.yaml \
  --backends spacetimedb
```

### Override the output directory

```bash
python3 bench_cross_db.py --config bench_cross_db_config.yaml \
  --output-dir ./results_2026_07_22
```

## Two-machine (remote) mode

By default the database servers and the benchmark clients run on the same
machine. Remote mode splits them: the **client machine** runs the scan (one
command, same as local mode), and the database servers are started and
stopped on the **server machine** over SSH, one clean instance per run —
exactly the same start/stop semantics as local mode, so no schema reset
logic is involved. All backends (postgres, postgres-procedure, mysql, mududb
interactive + procedure, spacetimedb) are supported.

### Prerequisites

- **Server machine**: a Rust toolchain (`cargo`, with `~/.cargo/bin` on the
  login-shell PATH — remote commands run through `bash -l`), the
  PostgreSQL/MySQL server binaries, the SpacetimeDB CLI requirements
  (auto-installed on the server as usual), and Python 3 with `PyYAML`. The
  source checkout and the release `mudud` binary are **not** needed up
  front — `--setup-remote` (below) syncs the source tree and builds mudud
  on the server. The wasm toolchain is **not** needed on the server: the
  mudud mpk is built on the client and pushed over HTTP. Note that the
  remote cargo build needs crates.io access from the server machine.
- **Client machine**: everything needed for local mode, plus the optional
  dependency `paramiko`:

```bash
pip install --user paramiko
```

- SSH access from the client to the server (key-based recommended).

### First-time setup and normal usage

Run `--setup-remote` once (and again after local code changes): it packs
the local source tree — including uncommitted changes, excluding `target/`,
`.git/`, benchmark work/result dirs and other bulky outputs — into a
tar.gz, uploads it via SFTP, extracts it at `remote.server_project_root`
and runs `cargo build --release -p mudud` on the server:

```bash
python3 bench_cross_db.py --config remote.yaml --setup-remote   # first time / after code updates
python3 bench_cross_db.py --config remote.yaml                  # normal scan
```

When a scan enables mududb backends but the remote
`target/release/mudud` is missing, the script prints a hint at startup
asking you to run `--setup-remote` first (it does not sync automatically —
syncing plus compiling takes minutes and should be an explicit action).

### Configuration

Remote mode is enabled by adding a `remote` section with `server_host` set
(under `defaults:` — `remote` is an infrastructure section and cannot be
overridden per test); without it the script behaves exactly as before. See
`bench_remote_example.yaml` for a full template:

```yaml
defaults:
  remote:
    server_host: 192.168.1.10        # setting this enables remote mode
    ssh_user: ybbh                   # default: current user
    ssh_port: 22
    ssh_key_filename: ""             # default: agent and ~/.ssh keys
    ssh_password: ""                 # optional; prefer keys
    server_project_root: /home/ybbh/workspace/ybbh/mududb_p
    # Fixed ports (the client must know them in advance). mudud_tcp_port is
    # the base of a block of `cores` consecutive ports (tcp_multi_port).
    postgres_port: 55432
    mysql_port: 53306
    mudud_tcp_port: 54000
    mudud_http_port: 58080
    spacetimedb_port: 53000

  # The servers must listen on an address reachable from the client:
  mudud:
    listen_ip: "0.0.0.0"
  spacetimedb:
    listen_ip: "0.0.0.0"
```

PostgreSQL (`-h 0.0.0.0`, plus a trust `pg_hba.conf` host line) and MySQL
(`--bind-address=0.0.0.0`, plus a throwaway `'bench'@'%'` account) are
adjusted automatically by the script in remote mode. `cpu_cores` tiers are
applied on the server machine via `taskset` (the core count is probed over
SSH for the skip check); `client_cpu_cores` still applies to the client
machine.

The remote agent is launched through a login shell (`bash -l -c ...`), so
PATH additions from `~/.profile` on the server machine apply (e.g.
`/usr/lib/postgresql/<ver>/bin` for `initdb`/`pg_ctl`). If a server binary
is still not found, set its absolute path in the `postgres:`/`mysql:`
config sections.

### How it works

For every run the client opens an SSH session, pushes the effective config
to `<server_project_root>/example/tpcc/bench_remote_agent.yaml` via SFTP and
launches `python3 bench_cross_db.py --config bench_remote_agent.yaml
--server-run <backend> --cores N` on the server. The agent starts the
backend, prints `[ready]`, and blocks on stdin; the client waits for the
fixed port, runs the benchmark, then sends `stop`. If the SSH connection
drops, the agent's stdin hits EOF and it shuts the database down by itself
(watchdog), so no orphaned database is left on the server.

### A note on cross-machine latency

In interactive modes (postgres, mysql, mududb interactive) each transaction
issues several round trips between client and server, so cross-machine
numbers include the network RTT and are **not directly comparable** to
single-machine numbers. Procedure/reducer modes (postgres-procedure, mududb
procedure, spacetimedb — one round trip per transaction) are affected much
less.

## Configuration reference

| Key | Type | Description |
|---|---|---|
| `cpu_cores` | list of int | Physical core counts to test. The script builds a contiguous core mask starting at core 0, e.g. `4` becomes `taskset -c 0-3`. |
| `connections` | list of int | Client connection counts to test, from under-loaded to over-loaded. |
| `repeats` | int | How many times to repeat each `(cores, connections)` pair. Results are aggregated. |
| `warmup_operations` | int | Reserved for future use; currently ignored. |
| `warehouses` | int | Number of TPC-C warehouses. |
| `districts` | int | Districts per warehouse. |
| `customers` | int | Customers per district. |
| `items` | int | Number of items in the item catalog. |
| `operations` | int | Number of operations executed per benchmark run. |
| `payment_percent` | int | Percentage of payment transactions. |
| `new_order_percent` | int | Percentage of new-order transactions. |
| `postgres.initdb` | string | Path to PostgreSQL `initdb`. |
| `postgres.pg_ctl` | string | Path to PostgreSQL `pg_ctl`. |
| `postgres.psql` | string | Path to PostgreSQL `psql`. |
| `postgres.user` | string | PostgreSQL superuser name. |
| `postgres.password` | string | PostgreSQL password (may be empty). |
| `postgres.database` | string | Database name to create for the benchmark. |
| `postgres.ddl` | string | Path to the DDL file. Resolved relative to `example/tpcc` if relative. |
| `mysql.mysqld` | string | Path to `mysqld`. |
| `mysql.mysqladmin` | string | Path to `mysqladmin`. |
| `mysql.mysql` | string | Path to `mysql` client. |
| `mysql.user` | string | MySQL user name. |
| `mysql.password` | string | MySQL password (may be empty). |
| `mysql.database` | string | Database name to create for the benchmark. |
| `mysql.ddl` | string | Path to the DDL file. Use `./sql/ddl_mysql.sql` for InnoDB. |
| `mysql.work_dir` | string or null | Parent directory for MySQL data. If unset and an AppArmor `usr.sbin.mysqld` profile is detected (Ubuntu), the script falls back to `/var/tmp`, because the profile only lets `mysqld` write under `/var/lib/mysql` and the user-tmp paths (`/tmp`, `/var/tmp`) — the project work dir is rejected with `Permission denied`/`File exists` during `--initialize-insecure`. Set this to an AppArmor-allowed path to benchmark a specific device. |
| `mudud.modes` | mapping | Mode entries `<name>: {server_mode, interactive_mode}`; each entry becomes the backend key `mududb-<name>`. `server_mode`: `iouring` (default) or `tokio`; `interactive_mode`: `interactive` or `procedure` (required). |
| `mudud.partition_count` | int | Optional. Number of range partitions for warehouse-partitioned runs. Defaults to the server worker count; must be 1-50 (the whole partition rule is stored as one catalog row that must fit a 4 KiB page). More partitions reduce cross-warehouse relation sharing at the cost of more relation files. |
| `mudud.listen_ip` | string | IP address for MuduDB HTTP and TCP listeners. |
| `mudud.ring_entries` | int | io_uring completion queue size (only relevant for `iouring`). |
| `spacetimedb.version` | string | SpacetimeDB version to install and use (default `1.12.0`). |
| `spacetimedb.cli_path` | string | Path to an existing `spacetime` CLI binary. If set, it is used as-is and auto-install is skipped. |
| `spacetimedb.install_dir` | string | Directory for the auto-installed CLI. Defaults to `<project_root>/bench_cross_db_work/spacetime-cli`. |
| `spacetimedb.listen_ip` | string | IP address for the SpacetimeDB node listener. |
| `client_cpu_cores` | list of int or null | If set, the benchmark client is pinned to this many cores starting at `client_cpu_offset`. Use `null` to not pin the client. |
| `client_cpu_offset` | int | First core to use for the client. Keep it outside the server core range to avoid interference. |
| `work_dir` | string or null | Directory holding all DB data and logs for the run. If omitted, a fresh `run_*` directory is created under `<project_root>/bench_cross_db_work/` — a real, persistent filesystem. `/tmp` is deliberately not used because it is commonly tmpfs, which would benchmark an in-memory filesystem instead of durable storage. Set this explicitly to benchmark a specific storage device. |
| `output_dir` | string | Directory for JSON/CSV/PNG outputs and the copied Markdown doc. |
| `keep_data` | bool | If true, temporary database directories are kept for debugging. |
| `buffer_pool_ratio` | float | Fraction of system RAM assigned to PostgreSQL `shared_buffers` and MySQL `innodb_buffer_pool_size` (default `0.8`). MuduDB ignores it. |
| `benchmark_timeout_secs` | int | Per-run wall-clock limit for the benchmark client in seconds (default `3600`). A run exceeding it is killed and reported as timed out. Must comfortably exceed seed time plus the measured phase at the configured dataset scale. |
| `think_times_ms` | list of int | Per-terminal think time in milliseconds slept between transactions (`0` = off). Paces the offered load: excluded from per-op latency, included in wall-clock elapsed/throughput. Sweep dimension — each value gets its own run tier and appears in the `tps_vs_think_time.png` / `p99_vs_think_time.png` charts. Default `[11000]` — the mix-weighted mean of the TPC-C per-type mean think times (Clause 5.2.5.4: 12 s New-Order/Payment, 10 s Order-Status, 5 s Delivery/Stock-Level) under the driver's default mix. Remember to raise `benchmark_timeout_secs` accordingly. |
| `remote.server_host` | string | Enables two-machine (remote) mode: database servers run on this host, orchestrated over SSH. Unset = local mode. See [Two-machine (remote) mode](#two-machine-remote-mode). |
| `remote.ssh_user` | string | SSH user name (default: current user). |
| `remote.ssh_port` | int | SSH port (default `22`). |
| `remote.ssh_key_filename` | string | Explicit SSH private key path; default tries the agent and `~/.ssh` keys. |
| `remote.ssh_password` | string | SSH password (optional; prefer keys). |
| `remote.server_project_root` | string | Path of this repository's checkout on the server machine. Required in remote mode. |
| `remote.postgres_port` / `remote.mysql_port` / `remote.mudud_tcp_port` / `remote.mudud_http_port` / `remote.spacetimedb_port` | int | Fixed server-side ports for remote mode (defaults `55432` / `53306` / `54000` / `58080` / `53000`). `mudud_tcp_port` is the base of a block of `cores` consecutive ports. |

## CPU core limiting

On Linux the script uses **`taskset -c <core-list>`** to bind the database
server (and optionally the benchmark client) to a fixed set of physical cores.
This lets you measure scaling from 1 core up to the full socket.

For example, to restrict a process to the first two physical cores:

```bash
taskset -c 0-1 /path/to/server
```

If you need an alternative:

- **cgroups v2**: `systemd-run --property=AllowedCPUs=0-1 -- /path/to/server`
- **cpulimit**: throttles a process to a percentage of one CPU (not a hard
  affinity mask).

## Outputs

The script produces the following files in `output_dir`:

- `bench_cross_db.md`: this documentation, copied to the result directory.
- `results.json`: one record per raw benchmark run (raw `backend`/`mode` keys).
- `summary.csv`: aggregated TPS/P99 per configuration; the first column
  `label` carries the short display label (see the mapping above), the raw
  `backend`/`mode` columns are kept for machine processing.
- `tps_vs_connections.png`: TPS vs connections, subplot per core count.
- `p99_vs_connections.png`: P99 latency vs connections, subplot per core count.
- `tps_vs_cores.png`: TPS vs cores, subplot per connection count.
- `p99_vs_cores.png`: P99 latency vs cores, subplot per connection count.

## Troubleshooting

### `taskset: failed to set pid XXX's affinity`

`taskset` requires that the target cores exist. Make sure `cpu_cores` does not
exceed the number of physical cores reported by the script at startup.

### PostgreSQL fails to start with "could not create lock file"

Make sure the temporary directory is writable and that no stale PostgreSQL
process is holding the port. The script uses random free ports, but a previous
run left running with `--keep-data true` might still be active.

### MySQL fails with "BLOB/TEXT column used in key specification without a key length"

Switch the MySQL DDL path to `./sql/ddl_mysql.sql`, which uses
`VARCHAR(255)` for the primary key instead of `TEXT`.

### MuduDB interactive mode fails with a nested runtime panic

The sync topology fetch used to build a Tokio runtime on the calling thread
and panic when one already existed. It now fetches on a fresh thread, so
`--warehouse-partitioned` works in interactive mode.

### MuduDB procedure mode fails with "table warehouse already exists"

This happened when the packaged `tpcc.mpk` created non-partitioned tables on
installation and the benchmark's partitioned DDL conflicted. Procedure mode
now installs `tpcc_partitioned.mpk` (no packaged DDL); the schema is created
client-side after the partition placement, so the conflict no longer occurs.

### MuduDB install fails with "operation timed out"

On a debug server the wasm module compilation during app install can exceed
the management client's default 10s timeout. The script exports
`MUDU_CLI_HTTP_TIMEOUT_SECS=120` for benchmark runs; raise it if needed.

### Charts are not generated

Install `matplotlib`:

```bash
python3 -m pip install matplotlib
```

The script will continue without charts and only emit a warning.

## Extending the script

The benchmark backends are implemented as small classes (`PostgresBackend`,
`MySqlBackend`, `MududbBackend`) sharing the `BenchmarkBackend` interface. To
add another database:

1. Subclass `BenchmarkBackend`.
2. Implement `start(cores)`, `stop()`, `is_ready()`, `connection_env()`,
   `benchmark_mode()`, and `extra_args()`.
3. Register the backend in `build_backends()` and add it to the `--backends`
   CLI option.
