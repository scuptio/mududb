# backup

WAL archiving, base backup/PITR restore, and logical dump/restore.

## Role

Part of the server stack in `crates/db-kernel` (`server -> backup -> repl ->
worker -> xcontract`, see [ARCHITECTURE.md](../../../ARCHITECTURE.md)).
Consumed by `server` (which re-exports its modules), `mudu_runtime`, the
`mudu_kernel` facade, and command-line tooling such as `mudu_cli`; builds on
`repl`, `worker`, `wal`, `sql`, and `command`.

## Key modules

- `backup` — WAL archiving (a periodic per-worker task copying sealed WAL
  chunks to the archive directory), base-backup orchestration, and the backup
  worker registry.
- `logical_backup` — Full-database logical dump / restore: catalog metadata
  plus one CSV per table, reusing the `COPY TO` / `COPY FROM` data path.
- `restore` — Offline restore / point-in-time recovery: reconstructs a server
  data directory from a base backup plus archived WAL, replayed forward to a
  target LSN / time / xid stop point.

## Features

- `debug_trace` — enables extra debug tracing via `mudu_utils/debug_trace`.
