# mcli Client

`mcli` is the MuduDB command-line client. It talks to the server over two
channels:

- the **TCP protocol** for SQL, key-value and procedure data-plane commands;
- the **HTTP management API** for admin commands (app management, partition
  routing, replication, backup). Those admin commands are covered separately
  in [mcli Management Interface (HTTP)](mcli_admin.md).

## Global flags

| Flag | Default | Meaning |
|------|---------|---------|
| `--addr` | `127.0.0.1:9527` | TCP protocol endpoint. |
| `--http-addr` | `127.0.0.1:8300` | HTTP management endpoint. |
| `--table` | off | Force the interactive table UI for SQL results (pretty output only). |
| `--no-table` / `--no-tui` | off | Always print JSON; never launch the table UI. |
| `--compact` | off | Compact JSON (also disables the table UI). |

## Data-plane commands

`put`, `get`, `range`, `invoke`, and `app-invoke` create and close a temporary
session automatically for each command.

### SQL: `command`

Run a single SQL statement (or execute request) encoded as JSON over the TCP
protocol:

```bash
mcli --addr 127.0.0.1:9527 command --json '{"app_name":"demo","sql":"select 1"}'
```

A SQL result shaped like
`{ "columns": [...], "rows": [[...]], "affected_rows": 0, "error": null }` is
rendered as an interactive full-screen table by default when stdout is a TTY.

### Interactive SQL: `shell`

```bash
mcli --addr 127.0.0.1:9527 shell --app demo
```

`--app` (default `demo`) selects the app the queries run against. End each SQL
statement with `;`. Meta commands: `\q`, `\help`, `\app <name>`.

### Key-value: `put`, `get`, `range`

```bash
mcli --addr 127.0.0.1:9527 put --json '{"key":"user-1","value":"value-1"}'

mcli --addr 127.0.0.1:9527 get --json '{"key":"user-1"}'

mcli --addr 127.0.0.1:9527 range --json '{"start_key":"a","end_key":"z"}'
```

`range` scans the half-open key range `[start_key, end_key)`.

### Procedure invoke (raw): `invoke`

```bash
mcli --addr 127.0.0.1:9527 invoke --json '{
  "procedure_name": "app/mod/proc",
  "procedure_parameters": {"base64": "cGF5bG9hZA=="}
}'
```

### Procedure invoke (installed app): `app-invoke`

`app-invoke` needs both endpoints (`--addr` to resolve the package, `--http-addr`
to invoke through TCP):

```bash
mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app kv --module key_value --proc kv_read --json '{"user_key":"user-1"}'
```

## JSON input

The `--json` / `--json-file` flags on `command`, `put`, `get`, `range`,
`invoke`, and `app-invoke` accept a request body in three forms:

- `--json '<json>'` — inline JSON.
- `--json-file request.json` — read from a file.
- `--json-file -` — read from stdin.

`put`, `get`, and `range` accept ordinary JSON values for keys and values; the
client still creates and injects a temporary session automatically.

## Interactive table controls

When the SQL result table UI is active:

- `q` / `Esc` — quit
- `Up` / `Down`, `PgUp` / `PgDn` — scroll rows
- `Left` / `Right` — move column focus
- `g` / `G` — jump to top / bottom

## Admin commands

See [mcli Management Interface (HTTP)](mcli_admin.md) for `app-list`,
`app-install`, `app-detail`, `app-uninstall`, `server-topology`,
`partition-route`, `repl`, `backup`, `restore`, `pitr`, `dump`, and
`restore-logical`.
