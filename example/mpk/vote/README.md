# Vote Example

A voting application example for MuduDB: users register with a phone
number, create and manage votes (single-choice or multiple-choice, with a
deadline and a result-visibility rule), cast and withdraw votes, and query
results and voting history. The original requirements specification
(in Chinese) is in [`vote.requirement.spec.md`](vote.requirement.spec.md),
and the entity-relationship diagram is in [`er/vote.plantuml`](er/vote.plantuml).

## What it demonstrates

- Writing Mudu Procedures in Rust with the `/**mudu-proc**/` marker.
- Using `mudu_query` and `mudu_command` from `mududb::sys_interface::sync_api` inside procedures.
- Modeling entities (`users`, `votes`, `options`, `vote_actions`, `vote_choices`) with `mgen`-generated bindings.
- Returning structured results (`VoteResult`, `VoteHistoryItem`) from procedures.
- Building a `wasm32-wasip2` component and packaging it into a `.mpk` file with `mpm-build`.

## Procedures

| Procedure | Description |
|-----------|-------------|
| `create_user` | Register a user by phone number; returns the new user id. |
| `create_vote` | Create a vote with topic, type (`single` / `multiple`), max choices, end time, and visibility rule (`always` / `after_end`). |
| `add_option` | Add an option to a vote; returns the new option id. |
| `cast_vote` | Cast a vote for one or more options. |
| `withdraw_vote` | Withdraw the caller's vote. |
| `get_vote_result` | Read the current result of a vote. |
| `get_voting_history` | List the voting history of a user. |

## Prerequisites

Follow the setup in [`doc/en/how_to_start.md`](../../../doc/en/how_to_start.md). Make sure these tools are in `PATH`:

- `cargo` and the `wasm32-wasip2` target
- `cargo-make`
- `mgen`, `mtp`, `mpm-build`, `mudud`, `mcli`

## Build the `.mpk` package

From this directory:

```bash
cargo make package
```

This runs `mgen entity` plus `mgen check-sql` (against `sql/ddl.sql` and
`sql/type.sql`), `mtp` transpilation, a `wasm32-wasip2` release build, and
`mpm-build create`, producing `target/wasm32-wasip2/release/vote.mpk` under
the repository-root `target/`.

## Install and invoke

Start `mudud`, then install the package:

```bash
mcli --http-addr 127.0.0.1:8300 app-install \
  --mpk ../../../target/wasm32-wasip2/release/vote.mpk
```

Register a user and create a vote:

```bash
mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app vote --module vote --proc create_user \
  --json '{"phone": "13800000000"}'

mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app vote --module vote --proc create_vote \
  --json '{"creator_id": "<user-id>", "topic": "Best language", "vote_type": "single", "max_choices": 1, "end_time": 1893456000, "visibility_rule": "always"}'
```

Add options and cast a vote:

```bash
mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app vote --module vote --proc add_option \
  --json '{"vote_id": "<vote-id>", "option_text": "Rust"}'

mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app vote --module vote --proc cast_vote \
  --json '{"user_id": "<user-id>", "vote_id": "<vote-id>", "option_ids": ["<option-id>"]}'
```

## Files of interest

- `src/rust/procedure.rs` — the handwritten Rust procedures.
- `src/rust/*.rs` — the entity modules (`users`, `votes`, `options`, `vote_actions`, `vote_choices`, ...).
- `src/generated/` — the transpiler-generated async wrappers.
- `sql/ddl.sql` / `sql/type.sql` — the application schema and result-shape entities.
- `Makefile.toml` — the build pipeline.
- `package/package.cfg.json` — package metadata for `mpm-build create`.
