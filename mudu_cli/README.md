# mcli

`mcli` is the TCP protocol client CLI for MuduDB.

`put`, `get`, `range`, `invoke`, and `app-invoke` create and close a temporary session automatically for each command.

It talks directly to the server TCP protocol and exposes these operations:

- `command`
- `put`
- `get`
- `range`
- `invoke`
- `app-install`
- `app-invoke`

## Examples

Query:

```bash
mcli --addr 127.0.0.1:9000 command --json '{"app_name":"demo","sql":"select 1"}'
```

Put:

```bash
mcli put --json '{
  "key": {"user": "u1"},
  "value": {"score": 9}
}'
```

Get:

```bash
mcli get --json '{
  "key": {"user": "u1"}
}'
```

Range scan:

```bash
mcli range --json '{
  "start_key": "a",
  "end_key": "z"
}'
```

Invoke:

```bash
mcli invoke --json '{
  "procedure_name": "app/mod/proc",
  "procedure_parameters": {"base64": "cGF5bG9hZA=="}
}'
```

Install `.mpk` through the management HTTP API:

```bash
mcli app-install --mpk target/wasm32-wasip2/release/key-value.mpk
```

Invoke an installed procedure through the TCP protocol:

```bash
mcli app-invoke --app kv --module key_value --proc kv_read --json '{
  "user_key": "user-1"
}'
```

## JSON input

JSON request bodies can be supplied in three ways:

- `--json '<json>'`
- `--json-file request.json`
- `--json-file -` to read from stdin

Binary fields may be passed as ordinary JSON values or as:

```json
{"base64":"..."}
```
