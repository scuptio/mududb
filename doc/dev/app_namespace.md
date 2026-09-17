# Per-App Schema Namespaces (default schema `mududb`)

Every table in mududb lives in a **schema**, PostgreSQL-schema style. The
schema name is the **application name** from the package's
`package.cfg.json`, so two apps installed on one server can define the same
table names without colliding — the canonical case is `wallet-go` and
`wallet-py`, whose `sql/ddl.sql` files both define `users`, `wallets`,
`transactions`, `orders`, `profile` and `profile_tags`. Before per-app
schemas, the second install's initdb DDL failed with `EntityAlreadyExists`
in the single global catalog namespace; now both installs succeed and each
app sees only its own tables.

Requests that carry no application context — a missing, empty (`""`) or
sentinel (`"default"`) app name — resolve against the default schema
**`mududb`** (the PostgreSQL `public` analogue; see
[`app_schema.rs`](../../crates/db-kernel/mudu_kernel/src/contract/app_schema.rs)).

## Name resolution

Unqualified table names resolve through the search path
**`[current schema, mududb]`**:

1. An exact hit in the current schema wins.
2. On a miss, the lookup falls back to the default schema `mududb`.

The fallback is what keeps pre-existing tables (see
[migration](#migration-from-pre-namespace-catalogs)) and the internal
`_fs_object` table reachable from every app context.

Qualified `schema.table` references in DML (`SELECT` / `INSERT` / `UPDATE` /
`DELETE`), `DROP TABLE` and `COPY` resolve **exactly, with no fallback**:
`SELECT ... FROM mududb.users` from an app context fails with
table-not-found when `mududb` has no `users`, even if the app's own schema
does.

`CREATE TABLE` always lands in the **current schema** (the request's app
name), and the collision check is per `(schema, name)`: an app may create a
table whose name also exists in `mududb` — the app's table then shadows the
default-schema one for that app's unqualified reads.

Table identity stays the random u128 OID end to end (storage, WAL, RPC);
schemas only rekey the catalog's name maps and the binder/plan-cache
resolution. There is no per-schema storage layout.

## What each client kind sees

| Client kind | How the app name is supplied | Current schema |
|---|---|---|
| `mcli command --json '{"app_name":"demo","sql":"..."}'` | the `app_name` field (wire: `ClientRequest.app_name`) | `demo` |
| `mcli shell --app <name>` | `--app` flag; `\app <name>` switches mid-session | `<name>` |
| Procedure SQL and relation syscalls in a guest | the invoke's app context, threaded through the invoke `Context` | the app being invoked |
| initdb DDL of an installed package | `app=<name>` connection option (also honored by the deferred startup drain) | the installed app |
| Bench / SDK adapter connection string | `mudud://<addr>/<app>` (path segment) | `<app>` |
| Anything with app name `""`, `"default"` or unset | — | `mududb` |

> **Behavior change:** `mcli shell` defaults to `--app demo`, so shell SQL
> now resolves in schema `demo` (with read fallback to `mududb`). To work
> directly against the default schema, run `mcli shell --app default` or
> issue `\app default` inside the shell.

## Migration from pre-namespace catalogs

Catalogs written before per-app schemas decode into the default schema:
the new `schema` field on the catalog's table records has a serde default
of `mududb`, so every pre-existing table loads as `mududb.<table>`. Those
tables stay reachable from every app context through the search-path
fallback — no data migration is needed. Note the asymmetry this creates:
an app that later creates a table with the same name shadows the migrated
one for its own unqualified SQL (the migrated table is then only reachable
from that app via the qualified `mududb.<table>` form).

## v1 limits

- **`CREATE TABLE schema.t (...)` is a syntax error.** The grammar accepts
  only a bare identifier as the create-table name; to choose the schema,
  set the request's app context instead.
- **Qualified references to schemas whose names contain `-` cannot be
  written.** The grammar's identifier rules reject the bare form
  (`wallet-go.users` is a syntax error in DML/DROP/COPY). The double-quoted
  form (`"wallet-go".users`) parses, but the qualifier retains the quote
  characters and therefore does not match the app schema either. App code
  is unaffected: procedures use unqualified names, which resolve through
  the search path. (Underscore names such as `wallet_go.users` and
  `mududb.users` work fine.)
- **Uninstall does not drop the schema or its tables.** Removing an app
  unloads its package; the tables remain in the catalog under the app's
  schema name and are still reachable (qualified) from other contexts.
- **No cross-schema listing.** There is no `SHOW TABLES`-style enumeration
  across schemas in v1; the catalog is keyed by `(schema, name)` internally
  but exposes no namespace listing.
- **`fs_type`, partition-rule and partition-placement names remain global
  namespaces** — they are not schema-scoped.

## Proof test

The end-to-end scenario lives in
[`test_app_namespace.rs`](../../crates/db-kernel/testing/tests/test_app_namespace.rs):
`wallet-go.mpk` and `wallet-py.mpk` install on the same server, diverging
rows are written through each app's procedures, and the same unqualified
SQL text returns different rows per app name — before, across a restart,
and never from the default schema.
