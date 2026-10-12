# YCSB Through mududb

This target runs the YCSB example through the `standalone-adapter` TCP path.
`mudu_adapter` uses `mudu_cli` internally to connect to a running `mudud` backend.

## 1. Deploy Database and mudud

Prepare and start `mudud` first.
Make sure the target app is installed and the TCP protocol port is reachable.

Example connection string:

```bash
export MUDU_CONNECTION="mudud://127.0.0.1:9527/ycsb?http_addr=127.0.0.1:8300"
```

The trailing `ycsb` is the app name used for `query` and `command`.
If you want the benchmark client to use the async worker path, enable it with the benchmark flag:

```bash
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --enable-async
```

The benchmark also supports explicit transaction wrapping per benchmarked operation:

```bash
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --enable-transaction
```

By default, explicit transactions apply only to the run phase.
If you also want load-phase inserts wrapped in `begin transaction` / `commit transaction`, add:

```bash
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --enable-transaction \
  --transaction-load
```

Transaction wrapping is implemented at the benchmark runner layer.
The YCSB procedures still use the key/value syscall path for the actual benchmarked operations, and the runner issues the transaction control statements on the same session around each operation.

During both load and benchmark phases, YCSB first fetches backend topology from the management HTTP API.
It caches the persistent `partition_oid -> worker_oid` mapping locally, opens each session directly on the target worker OID, and uses the partition OID in benchmark key prefixes.

## 2. Prepare Test Program

Check the example and the benchmark runner:

```bash
cargo check -p ycsb
cargo check -p ycsb --features benchmark-runner
```

## 3. Execute Test Program

Workload A:

```bash
export MUDU_CONNECTION="mudud://127.0.0.1:9527/ycsb?http_addr=127.0.0.1:8300"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload a \
  --connection-count 10000 \
  --partition-count 16 \
  --record-count 20000 \
  --operation-count 20000
```

Workload B:

```bash
export MUDU_CONNECTION="mudud://127.0.0.1:9527/ycsb?http_addr=127.0.0.1:8300"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload b
```

Workload F:

```bash
export MUDU_CONNECTION="mudud://127.0.0.1:9527/ycsb?http_addr=127.0.0.1:8300"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload f \
  --enable-async \
  --connection-count 8 \
  --partition-count 8
```

Workload A with explicit transactions:

```bash
export MUDU_CONNECTION="mudud://127.0.0.1:9527/ycsb?http_addr=127.0.0.1:8300"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload a \
  --connection-count 8 \
  --partition-count 8 \
  --enable-transaction
```

## 4. Verify Backend State

Verify against the database or backend state used by the `ycsb` app behind `mudud`.

## 5. Cleanup

Stop `mudud` or reset the backend state if you want a fresh run.
