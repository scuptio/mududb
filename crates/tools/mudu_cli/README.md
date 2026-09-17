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
```

## JSON input

JSON request bodies can be supplied in three ways:

- `--json '<json>'`
- `--json-file request.json`
- `--json-file -` to read from stdin

`put`, `get`, and `range` accept ordinary JSON values for keys and values. `mcli` still creates and injects a temporary session automatically for those commands.
