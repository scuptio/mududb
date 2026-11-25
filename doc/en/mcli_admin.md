# mcli Management Interface (HTTP)

This document describes how to use `mcli` management commands for app management, partition routing, replication administration, and backup/recovery. For the data-plane commands (`command`, `shell`, `put`, `get`, `range`, `invoke`, `app-invoke`) and JSON input/output flags, see [mcli Client](mcli.md).

## Scope

- Applies when `mcli` connects to `mudud` management endpoint via `--http-addr`.
- Management commands run over the HTTP API, without manual HTTP calls.
- If the server does not expose admin capabilities, commands return an error (for example, `admin service is not configured`).

## Prerequisites

1. Start `mudud`.
2. Ensure the HTTP endpoint is reachable (default `127.0.0.1:8300`).

## Command Reference

### 1) List installed applications

```bash
mcli --http-addr 127.0.0.1:8300 app-list
```

### 2) Install an application package (`.mpk`)

```bash
mcli --http-addr 127.0.0.1:8300 app-install --mpk target/wasm32-wasip2/release/wallet.mpk
```

### 3) Show procedures of an application

```bash
mcli --http-addr 127.0.0.1:8300 app-detail --app wallet
```

### 4) Show one procedure detail

```bash
mcli --http-addr 127.0.0.1:8300 app-detail --app wallet --module wallet --proc create_user
```

### 5) Uninstall an application

```bash
mcli --http-addr 127.0.0.1:8300 app-uninstall --app wallet
```

### 6) Show server topology

```bash
mcli --http-addr 127.0.0.1:8300 server-topology
```

### 7) Partition route query

Route by exact key:

```bash
mcli --http-addr 127.0.0.1:8300 partition-route --rule-name user_rule --key user-100
```

Route by range (supports comma-delimited composite values):

```bash
mcli --http-addr 127.0.0.1:8300 partition-route --rule-name user_rule --start 100 --end 200
```

Composite key example:

```bash
mcli --http-addr 127.0.0.1:8300 partition-route --rule-name order_rule --key tenant-1,order-100
```

### 8) Replication administration (`repl`)

```bash
mcli --http-addr 127.0.0.1:8300 repl add-node --node-id 4 --host 10.0.0.4 --port 19530 --learner
mcli --http-addr 127.0.0.1:8300 repl promote-learner --node-id 4
mcli --http-addr 127.0.0.1:8300 repl remove-node --node-id 4
mcli --http-addr 127.0.0.1:8301 repl promote --fencing-cmd ./fence-old-primary.sh
mcli --http-addr 127.0.0.1:8301 repl promote --yes
mcli --http-addr 127.0.0.1:8301 repl follow --primary 10.0.0.2:19527 --primary-node-id 2
mcli --http-addr 127.0.0.1:8300 repl status
```

`repl add-node` joins as a voter by default and as a learner with `--learner`.
`repl promote` requires either `--fencing-cmd` or an explicit `--yes`.

### 9) Base backup administration (`backup`)

Requires `[backup] backup_dir` in `mudud.cfg`.

```bash
mcli --http-addr 127.0.0.1:8300 backup create --label daily-1
mcli --http-addr 127.0.0.1:8300 backup list
mcli --http-addr 127.0.0.1:8300 backup verify --label daily-1
mcli --http-addr 127.0.0.1:8300 backup delete --label daily-1
```

### 10) Restore and point-in-time recovery (offline)

`restore` materializes a base backup plus archived WAL into a target directory;
`pitr` recovers to a specific `--lsn`, `--time` (millis), or `--xid`.

```bash
mcli restore --label daily-1 --backup-dir /backups --archive-dir /wal-archive --target-dir /restored
mcli pitr --label daily-1 --backup-dir /backups --archive-dir /wal-archive --target-dir /recovered --time 1700000000000
```

### 11) Logical dump and restore

```bash
mcli --http-addr 127.0.0.1:8300 dump --label daily-1 --dir /dumps
mcli --http-addr 127.0.0.1:8300 restore-logical --label daily-1 --dir /dumps
mcli --http-addr 127.0.0.1:8300 restore-logical --label daily-1 --dir /dumps --drop
```

## Argument Constraints

- For `app-detail`:
  - `--proc` must be used together with `--module`.
- For `partition-route`:
  - `--key` and `--start/--end` are mutually exclusive.
  - You must provide `--key`, or at least one of `--start` / `--end`.

## Output

- Default output is pretty JSON.
- Use `--compact` for compact JSON.
- On failure, the command exits non-zero and prints the error to stderr.
