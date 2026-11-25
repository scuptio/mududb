# HTTP Management API Reference

The `mudud` server exposes a management HTTP API on the port configured by
`http_listen_port` (default `8300`). All management paths are under `/mudu`;
the WebSocket gateway is `/ws`. `mcli` wraps most of these endpoints
(see [mcli Management Interface](mcli_admin.md)); the table below lists the
raw method + path so the API can be driven directly.

Capability gates (from the server backend's capability set — the kernel/IOUring
backend enables invoke, uninstall and replication admin; the legacy backend
enables invoke only):

- `enable_invoke` — gates `POST /mudu/app/invoke/...` and `GET /ws`.
- `enable_uninstall` — gates `DELETE /mudu/app/uninstall/...`.
- `enable_repl_admin` — gates the `/mudu/repl/*` endpoints.

## App management

| Method | Path | Purpose | mcli |
|---|---|---|---|
| GET | `/mudu/app/list` | List installed application names. | `app-list` |
| GET | `/mudu/app/list/{app}` | List an app's procedures (`module/proc` pairs). | `app-detail --app` |
| GET | `/mudu/app/list/{app}/{module}/{proc}` | Procedure detail (descriptor + default param/return JSON). | `app-detail --app --module --proc` |
| POST | `/mudu/app/install` | Install an `.mpk` package (binary body). | `app-install --mpk` |
| DELETE | `/mudu/app/uninstall/{app}` | Uninstall an app (`enable_uninstall`). | `app-uninstall --app` |
| POST | `/mudu/app/invoke/{app}/{module}/{proc}` | Invoke a procedure with a JSON body (`enable_invoke`). | `app-invoke` |

## Topology and replication

| Method | Path | Purpose | mcli |
|---|---|---|---|
| GET | `/mudu/server/topology` | Worker/replication topology view. | `server-topology` |
| GET | `/mudu/server/replication` | Replication status view. | `repl status` |
| POST | `/mudu/repl/membership` | Raft membership change (`enable_repl_admin`). | `repl add-node` / `remove-node` / `promote-learner` |
| POST | `/mudu/repl/promote` | Promote this node to stream primary (`enable_repl_admin`). | `repl promote` |
| POST | `/mudu/repl/follow` | Point this node at a stream primary (`enable_repl_admin`). | `repl follow` |

## Partition routing and maintenance

| Method | Path | Purpose | mcli |
|---|---|---|---|
| POST | `/mudu/partition/route` | Route a point key or key range to its partitions/workers. | `partition-route` |
| POST | `/mudu/cluster/partition/move` | Move a partition to another node group (cluster mode). | — |
| POST | `/mudu/cluster/partition/abort_move` | Abort an in-flight partition move (cluster mode). | — |
| POST | `/mudu/cluster/partition/split` | Split a partition at a split point (cluster mode). | — |
| POST | `/mudu/cluster/partition/merge` | Merge two adjacent partitions (cluster mode). | — |

The `cluster/partition/*` request bodies are documented in
[Partition Maintenance Operations](partition.md#partition-maintenance-operations).

## Queues

| Method | Path | Purpose | mcli |
|---|---|---|---|
| GET | `/mudu/server/queues` | Per-queue runtime status (depth, bytes, waiters, consumers, DLQ, activation). | — |
| POST | `/mudu/server/queue_activation` | Register a queue activation (WASM procedure invoked when a queue is non-empty). | — |
| DELETE | `/mudu/server/queue_activation/{queue}` | Unregister a queue activation. | — |
| GET | `/mudu/server/queue_activations` | List queue activations. | — |

Queue receive (long-poll) is reachable only through the
[WebSocket gateway](websocket_gateway.md), not as a plain HTTP route.

## Backup and recovery

| Method | Path | Purpose | mcli |
|---|---|---|---|
| POST | `/mudu/backup` | Take a base backup under a label. | `backup create --label` |
| GET | `/mudu/backup/list` | List backup labels. | `backup list` |
| POST | `/mudu/backup/verify` | Verify a backup's integrity. | `backup verify --label` |
| POST | `/mudu/backup/delete` | Delete a base backup. | `backup delete --label` |
| POST | `/mudu/dump` | Write a logical dump (schema + CSV) into a directory. | `dump --label --dir` |
| POST | `/mudu/restore-logical` | Restore a logical dump (optional `drop_existing`). | `restore-logical --label --dir [--drop]` |

`restore` and `pitr` are offline `mcli` commands that materialize a backup
locally; they do not go through a server HTTP endpoint.

## WebSocket gateway

| Method | Path | Purpose |
|---|---|---|
| GET | `/ws` | Procedure invocation + zone-scoped queue receive fan-out (`enable_invoke`). See [WebSocket Gateway](websocket_gateway.md). |
