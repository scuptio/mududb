# Partitioning

This document describes the current partition mechanism in MuduDB.

The implementation is based on four layers:

- a global range partition rule
- a table-to-rule binding
- a partition-to-worker placement
- a routing layer that maps SQL operations to physical partitions

The goal is to let multiple tables share the same range layout while allowing partitions to be distributed across
multiple workers.

## Core Concepts

### Partition Rule

A partition rule is a global metadata object.

It defines:

- the partitioning method, currently `RANGE`
- one or more partition key columns
- an ordered set of partition boundaries
- a list of logical partitions

Each rule produces multiple logical partitions. These partitions are later assigned to workers by placement metadata.

### Table Binding

A table does not store the full partition layout in its schema.

Instead, a table binds to an existing global rule and declares which table columns reference that rule.

This lets multiple tables reuse the same partition layout.

In the current implementation, the referenced columns are expected to match the primary-key prefix. This keeps point
lookup, range pruning, and storage routing aligned with the existing key encoding model.

### Placement

Placement maps each logical partition to a worker.

This is a separate metadata layer because:

- placement is a deployment concern, not a schema concern
- placement may change independently of the table definition
- the same rule may be reused by multiple tables

### Physical Relation

Inside a worker, relation storage is not keyed only by `table_id`.

It is keyed by the physical pair:

```text
(table_id, partition_id)
```

This is required because one worker may own multiple partitions of the same logical table.

## Metadata Model

The partition metadata is split into three object types.

### `PartitionRuleDesc`

Defines a global range rule:

- `rule_id`
- `name`
- `kind`
- `key_types`
- `partitions`
- `version`

Each partition entry contains:

- `partition_id`
- `name`
- `start` bound, inclusive
- `end` bound, exclusive

### `TablePartitionBinding`

Defines how a table uses a global rule:

- `table_id`
- `rule_id`
- `ref_attr_indices`

`ref_attr_indices` identifies the table columns that form the partition key.

### `PartitionPlacement`

Defines where a partition lives:

- `partition_id`
- `worker_id`

## Catalogs

Partition metadata is persisted in dedicated internal catalogs.

- `__meta_partition_rule`
- `__meta_table_partition_binding`
- `__meta_partition_placement`

These catalogs are managed by `MetaMgr` and cached in memory by the metadata layer.

## DDL

The current implementation supports the following statements.

### Create a Global Partition Rule

```sql
CREATE PARTITION RULE r_orders
RANGE (region_id, order_id) (
  PARTITION p0 VALUES FROM (MINVALUE, MINVALUE) TO (1000, MINVALUE),
  PARTITION p1 VALUES FROM (1000, MINVALUE) TO (2000, MINVALUE),
  PARTITION p2 VALUES FROM (2000, MINVALUE) TO (MAXVALUE, MAXVALUE)
);
```

### Create a Partitioned Table

```sql
CREATE TABLE orders (
  region_id BIGINT,
  order_id BIGINT,
  amount BIGINT,
  PRIMARY KEY(region_id, order_id)
)
PARTITION BY GLOBAL RULE r_orders
REFERENCES (region_id, order_id);
```

### Create Partition Placement

```sql
CREATE PARTITION PLACEMENT FOR RULE r_orders (
  PARTITION p0 ON WORKER 1,
  PARTITION p1 ON WORKER 2,
  PARTITION p2 ON WORKER 3
);
```

## Routing Model

Routing is implemented by `PartitionRouter`.

For a partitioned table, the router performs these steps:

1. Load the table binding.
2. Load the referenced partition rule.
3. Extract the partition key from the SQL key tuple.
4. Compare the key tuple with partition bounds.
5. Resolve the target `partition_id`.
6. Resolve the target `worker_id` from placement metadata.

### Point Operations

Point `INSERT`, `READ`, `UPDATE`, and `DELETE` are routed to one partition.

The routing key is built from the bound partition columns, then compared to the range rule.

### Range Operations

Range reads perform partition pruning.

The router finds all partitions whose ranges overlap the requested key range. The engine then:

- scans matching local partitions directly
- forwards requests for remote partitions to the owning worker
- merges the returned rows

## Worker Storage Model

`WorkerStorage` stores relation data by physical relation identity rather than logical table identity.

This changes the storage model from:

```text
table_id -> relation
```

to:

```text
(table_id, partition_id) -> relation
```

Relations are created lazily for the partitions that the worker needs to access.

## Transaction and WAL Model

Partitioned writes require transaction state and WAL to carry physical partition identity.

The current implementation uses a shared `PhysicalRelationId`:

```text
{ table_id, partition_id }
```

This identity is used by:

- transaction staging
- write conflict detection
- commit-time write locking
- WAL records for relation insert and delete
- WAL replay

This avoids corrupting data when multiple partitions of the same table exist on the same worker.

## Remote Partition Access

The engine currently supports partition RPC over the worker message bus.

Supported remote actions:

- point read
- range read
- insert
- update
- delete

Remote requests are routed by partition placement and executed by the worker that owns the target partition.

## Port Sharding

Partition placement decides which worker owns a logical partition. Port sharding exposes those workers through stable
TCP ports so clients and tools can connect directly to the worker that owns the data they want to use.

The current IOUring and Tokio backends use this policy:

- `tcp_listen_port` is the base TCP port
- worker `0` listens on `tcp_listen_port`
- worker `1` listens on `tcp_listen_port + 1`
- worker `N` listens on `tcp_listen_port + N`

When the backend has more than one worker, `tcp_multi_port` is enabled by the backend startup path. For example, with
`tcp_listen_port = 9527` and four workers, the worker ports are:

```text
worker 0 -> 9527
worker 1 -> 9528
worker 2 -> 9529
worker 3 -> 9530
```

This is a transport-level sharding mechanism. It does not replace partition placement. Placement still maps
`partition_id` to `worker_id`; port sharding maps the selected worker to a reachable TCP endpoint.

### Discovering Worker Ports

Use the HTTP management API to read the live topology:

```bash
mcli --http-addr 127.0.0.1:8300 server-topology
```

The response includes:

- `worker_count`
- `tcp_multi_port`
- `tcp_base_listen_port`
- each worker's `worker_index`
- each worker's `worker_id`
- each worker's `tcp_listen_port`
- the partition ids currently associated with that worker

Use `partition-route` to resolve a rule key or range to partition and worker ids:

```bash
mcli --http-addr 127.0.0.1:8300 partition-route --rule-name r_orders --key 1001,50001
```

A client can combine both calls:

1. `partition-route` resolves the target `partition_id` and `worker_id`.
2. `server-topology` resolves that `worker_id` to `tcp_listen_port`.
3. The client connects to `listen_ip:tcp_listen_port`.

### Why It Exists

Port sharding is useful when the caller can choose the target partition before opening or rebinding a session.

It enables:

- direct client affinity to the owning worker
- fewer cross-worker forwards for partition-local traffic
- stable per-worker endpoints for benchmark and operational tooling
- an external way to inspect how placement maps onto network endpoints

### Limits

Port sharding is currently tied to the multi-worker IOUring and Tokio backend paths. The legacy backend keeps the older
single TCP/PG serving model and should not be treated as a per-worker port-sharded deployment.

Clients still need to respect partition placement. Connecting to a worker port only selects an execution worker; it
does not move data or change which worker owns a partition.

## Current Semantics and Limits

The current implementation is intentionally scoped.

- only `RANGE` partitioning is supported
- partition bindings are expected to match the primary-key prefix
- partition pruning is based on key columns, not arbitrary predicates
- placement is explicit metadata
- remote partition access uses worker-to-worker RPC
- per-worker port sharding is available on the multi-worker IOUring and Tokio backend paths

There is still an important transactional limit:

- cross-worker writes are forwarded and executed remotely, but there is no distributed two-phase commit yet

This means full atomic commit across multiple workers is not implemented. The current model is suitable for routed
single-partition writes and for cross-partition reads, but it is not yet a complete distributed transaction protocol.

## Recommended Usage

Use partitioning when:

- tables are naturally partitioned by ordered keys
- point lookups and range scans follow the same key prefix
- data should be spread across multiple workers
- a single logical partition layout should be reused by multiple tables

Avoid using the current implementation as if it were already a general distributed transaction engine.

## Summary

The current partition subsystem separates:

- logical partition definition
- table binding
- worker placement
- physical storage
- runtime routing
- optional per-worker TCP port sharding

This keeps the schema model separate from deployment topology while still allowing clients and tools to discover the
worker endpoint that owns a routed partition.

This keeps the schema model clean and allows the engine to evolve toward partition rebalance, partition split or merge,
and distributed commit in later iterations.
