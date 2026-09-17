# YCSB on SQLite

This target is the lightest way to run the YCSB example.
It uses the `standalone-adapter` path through `sys_interface` and stores data in a local SQLite file.

## 1. Deploy Database

No separate database server is required.
The adapter creates the SQLite file and internal tables automatically.

You only need to choose the database file path:

```bash
export MUDU_CONNECTION="sqlite://./ycsb.db"
```

If you want a different location:

```bash
export MUDU_CONNECTION="sqlite:///tmp/ycsb.db"
```

## 2. Prepare Test Program

Check the example and the local benchmark runner:

```bash
cargo check -p ycsb
cargo check -p ycsb --features benchmark-runner
```

## 3. Execute Test Program

Workload A:

```bash
export MUDU_CONNECTION="sqlite://./ycsb.db"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload a \
  --record-count 10000 \
  --operation-count 10000
```

Workload B:

```bash
export MUDU_CONNECTION="sqlite://./ycsb.db"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload b
```

Workload F:

```bash
export MUDU_CONNECTION="sqlite://./ycsb.db"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload f
```

## 4. Verify Output

The runner prints:

- workload name
- record count
- operation count
- load time
- run time
- throughput
- operation counters by type

## 5. Cleanup

Remove the database file if you want a fresh run:

```bash
rm -f ./ycsb.db
```
