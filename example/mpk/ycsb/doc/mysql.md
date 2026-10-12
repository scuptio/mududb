# YCSB on MySQL

This target runs the YCSB example against MySQL through `mudu_adapter`.
It uses the `standalone-adapter` path, stores KV data in MySQL, and keeps session state in the local adapter process.

## 1. Deploy Database

Create a MySQL database first.

Example:

```bash
mysql -uroot -proot -e "CREATE DATABASE ycsb;"
```

Set the connection string:

```bash
export MUDU_CONNECTION="mysql://root:root@127.0.0.1:3306/ycsb"
```

The adapter creates this table automatically if it does not exist:

- `mudu_kv`

Session state is kept in the local adapter process and is not persisted in MySQL.

## 2. Prepare Test Program

Check the example and the benchmark runner:

```bash
cargo check -p ycsb
cargo check -p ycsb --features benchmark-runner
```

## 3. Execute Test Program

Workload A:

```bash
export MUDU_CONNECTION="mysql://root:root@127.0.0.1:3306/ycsb"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload a \
  --record-count 10000 \
  --operation-count 10000
```

Workload C:

```bash
export MUDU_CONNECTION="mysql://root:root@127.0.0.1:3306/ycsb"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload c
```

Workload F:

```bash
export MUDU_CONNECTION="mysql://root:root@127.0.0.1:3306/ycsb"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload f
```

## 4. Verify Database State

Inspect the internal tables:

```bash
mysql -uroot -proot ycsb -e "SELECT COUNT(*) AS kv_rows FROM mudu_kv;"
```

## 5. Cleanup

Drop the database when finished:

```bash
mysql -uroot -proot -e "DROP DATABASE ycsb;"
```
