# mudu_client

TCP/JSON client and management HTTP API library for MuduDB.

- `client`: synchronous and asynchronous clients for the MuduDB wire protocol,
  plus a JSON-facing wrapper (`SyncClient`, `AsyncClient` / `AsyncClientImpl`,
  `JsonClient`).
- `management`: HTTP helpers for the MuduDB management endpoints (app
  lifecycle, server topology, partition routing).

Used by the `mcli` CLI (`mudu_cli` crate) and embeddable by other tools that
need to talk to a MuduDB server without pulling in terminal-UI dependencies.
