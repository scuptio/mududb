# YCSB on PostgreSQL

This target runs the YCSB example against PostgreSQL through `mudu_adapter`.
It uses the `standalone-adapter` path, stores KV data in PostgreSQL, and keeps session state in the local adapter process.

## 1. Deploy Database

Create a PostgreSQL database first.

Example with local PostgreSQL:

```bash
createdb ycsb
```

Or with `psql`:

```bash
psql -U postgres -c "CREATE DATABASE ycsb;"
```

Set the connection string:

```bash
export MUDU_CONNECTION="postgres://postgres:postgres@127.0.0.1:5432/ycsb"
```

The adapter creates this table automatically if it does not exist:

- `mudu_kv`

Session state is kept in the local adapter process and is not persisted in PostgreSQL.

## 2. Prepare Test Program

Check the example and the benchmark runner:

```bash
cargo check -p ycsb
cargo check -p ycsb --features benchmark-runner
```

## 3. Execute Test Program

Workload A:

```bash
export MUDU_CONNECTION="postgres://postgres:postgres@127.0.0.1:5432/ycsb"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload a \
  --record-count 10000 \
  --operation-count 10000
```

Workload B:

```bash
export MUDU_CONNECTION="postgres://postgres:postgres@127.0.0.1:5432/ycsb"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload b
```

Workload E:

```bash
export MUDU_CONNECTION="postgres://postgres:postgres@127.0.0.1:5432/ycsb"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload e \
  --scan-length 100
```

## 4. Verify Database State

Inspect the internal tables:

```bash
psql -U postgres -d ycsb -c "SELECT COUNT(*) FROM mudu_kv;"
```

## 5. Cleanup

Drop the database when finished:

```bash
dropdb ycsb
```
