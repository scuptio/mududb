# mcli

`mcli` is the CLI client for MuduDB.

`put`, `get`, `range`, `invoke`, and `app-invoke` create and close a temporary session automatically for each command.

It talks to the server over:

- TCP protocol for SQL/KV/procedure data-plane commands
- HTTP API for admin commands

It exposes these operations:

- `command`
- `shell`
- `put`
- `get`
- `range`
- `invoke`
- `app-install`
- `app-invoke`
- `app-list`
- `app-detail`
- `app-uninstall`
- `server-topology`
- `partition-route`
- `repl` (replication cluster administration)
  - `repl add-node --node-id N --host H --port P [--learner]` — raft: add a member (voter by default, learner with `--learner`)
  - `repl remove-node --node-id N` — raft: remove a member
  - `repl promote-learner --node-id N` — raft: promote a caught-up learner to voter
  - `repl promote [--fencing-cmd <script> | --yes]` — stream: promote this node to primary; `--fencing-cmd` runs locally first and aborts on failure, otherwise `--yes` is the explicit fencing confirmation
  - `repl follow --primary <host:port> --primary-node-id <N>` — stream: point this node at a new primary (verifies the target's node id)
  - `repl status [--table]` — replication status of a node (mode/role/leader-or-primary, per-worker watermarks, subscriber lag, storage usage, operational counters); JSON by default, sectioned text with `--table`

`repl` commands go through the HTTP replication admin endpoints.
Membership commands must reach the raft leader; when the contacted node is
not the leader, the client automatically re-issues the request against the
`leader_hint` returned in the error envelope (up to 4 redirects). Stream
failover flow: fence the old primary → `repl promote` on the chosen
replica (epoch advances, write gate opens) → `repl follow` on every other
replica (sinks reconnect, no restart). The new epoch is the data-plane
backstop: a zombie old primary's lower-epoch stream is rejected.

## Examples

Query:

```bash
mcli --addr 127.0.0.1:9527 command --json '{"app_name":"demo","sql":"select 1"}'
```

Interactive shell:

```bash
mcli --addr 127.0.0.1:9527 shell --app demo
```

## Output (SQL query)

When `mcli command` returns a SQL result shaped like:

```json
{ "columns": ["..."], "rows": [["..."]], "affected_rows": 0, "error": null }
```

`mcli` renders it as an interactive full-screen table by default when stdout is a TTY.

Controls:

- `q` / `Esc`: quit
- `Up` / `Down`, `PgUp` / `PgDn`: scroll rows
- `Left` / `Right`: move column focus
- `g` / `G`: jump to top / bottom

Flags:

- `--table`: force table UI (only applies to pretty output)
- `--no-table` (alias: `--no-tui`): always print JSON
- `--compact`: compact JSON (also disables table UI)

Put:

```bash
mcli --addr 127.0.0.1:9527 put --json '{
  "key": "user-1",
  "value": "value-1"
}'
```

Get:

```bash
mcli --addr 127.0.0.1:9527 get --json '{
  "key": "user-1"
}'
```

Range scan:

```bash
mcli --addr 127.0.0.1:9527 range --json '{
  "start_key": "a",
  "end_key": "z"
}'
```

Invoke:

```bash
mcli --addr 127.0.0.1:9527 invoke --json '{
  "procedure_name": "app/mod/proc",
  "procedure_parameters": {"base64": "cGF5bG9hZA=="}
}'
```

Install `.mpk` through the HTTP management API:

```bash
mcli --http-addr 127.0.0.1:8300 app-install --mpk target/wasm32-wasip2/release/key-value.mpk
```

Invoke an installed procedure through the TCP protocol:

```bash
mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke --app kv --module key_value --proc kv_read --json '{
  "user_key": "user-1"
}'
```

Management commands:

```bash
mcli --http-addr 127.0.0.1:8300 app-list
mcli --http-addr 127.0.0.1:8300 app-detail --app wallet
mcli --http-addr 127.0.0.1:8300 app-detail --app wallet --module wallet --proc create_user
mcli --http-addr 127.0.0.1:8300 app-uninstall --app wallet
mcli --http-addr 127.0.0.1:8300 server-topology
mcli --http-addr 127.0.0.1:8300 partition-route --rule-name user_rule --key user-100
mcli --http-addr 127.0.0.1:8300 repl add-node --node-id 4 --host 10.0.0.4 --port 19527 --learner
mcli --http-addr 127.0.0.1:8300 repl promote-learner --node-id 4
mcli --http-addr 127.0.0.1:8300 repl remove-node --node-id 4
mcli --http-addr 127.0.0.1:8301 repl promote --fencing-cmd ./fence-old-primary.sh
mcli --http-addr 127.0.0.1:8301 repl promote --yes
mcli --http-addr 127.0.0.1:8301 repl follow --primary 10.0.0.2:19527 --primary-node-id 2
mcli --http-addr 127.0.0.1:8300 repl status
mcli --http-addr 127.0.0.1:8300 repl status --table
```

## JSON input

JSON request bodies can be supplied in three ways:

- `--json '<json>'`
- `--json-file request.json`
- `--json-file -` to read from stdin

`put`, `get`, and `range` accept ordinary JSON values for keys and values. `mcli` still creates and injects a temporary session automatically for those commands.
