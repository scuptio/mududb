# SQL Subset and Static Checking (`mgen check-sql`)

Mudu guests issue SQL as plain string literals. `mgen check-sql` statically
checks those literals against the DDL schema at build time, catching broken
table/column references, bind-parameter mismatches, and `SELECT` result-shape
mismatches before the package ever reaches a database.

## The supported SQL subset

The checker is built on the same parser (`sql_parser`) the engine uses, so
"supported" means "the engine can actually execute it":

- single-table `SELECT` with a column list, `*`, or single-argument
  aggregate calls (`COUNT(*)`, `SUM(col)`), optional alias, optional
  `WHERE`;
- `INSERT INTO t (cols..) VALUES (..), (..), ..`;
- `UPDATE t SET col = value, .. WHERE ..` (values may be literals, `?`
  placeholders, or arithmetic like `col = col + 1`);
- `DELETE FROM t WHERE ..`;
- `WHERE` predicates are comparisons (`=`, `<>`, `<`, `<=`, `>`, `>=`)
  combined with `AND`;
- `?` parameter placeholders everywhere a value is accepted.

Identifiers (table and column names) are matched ASCII case-insensitively,
as in SQL.

Valid SQL outside this subset — `JOIN`, subqueries, `GROUP BY` / `ORDER BY`
/ `LIMIT`, set operations, `IN` / `EXISTS` / `LIKE`, multi-argument
functions, and so on — is *not* rejected; see the severity model below.

## Named parameters (`:name`)

Besides `?` positional placeholders, SQL may use named placeholders:
`:identifier` (a colon followed by `[A-Za-z_][A-Za-z0-9_]*`). They are
resolved **host-side**, so every guest language benefits uniformly.

Syntax and semantics:

- `''`/`""` string literals, `--` line comments, `/* */` block comments,
  and PostgreSQL-style casts (`::type`) are never placeholders.
- Named and positional placeholders must not be mixed in one statement.
- Resolution happens in the adapter (`param_binder::resolve_named_sql`):
  the SQL is rewritten to positional form in occurrence order, and values
  are expanded per occurrence — a repeated `:name` reuses its value at
  every occurrence.
- Guests send the names alongside the values in the `param-names` wire
  field (see the ABI guide); the values are matched to placeholders by
  name, not position.
- Errors, each with a clear message: a `:name` missing from
  `param-names`; a `param-names` entry never used in the SQL;
  `param-names` present but no `:name` in the SQL; `:name` present but no
  `param-names`; a names/values count mismatch; `?`/`:name` mixing.

check-sql compatibility: the vendored grammar cannot parse `:name`, so
the check driver runs the same scanner first and checks the **rewritten**
positional SQL (table/column existence, arity against the occurrence
count, literal type tags all keep working). Diagnostics for named
literals point into the rewritten text.

## Diagnostic severities

Every diagnostic is one of:

- **error** — the statement is invalid and would fail at runtime: syntax
  errors, unknown tables, unknown columns, wrong number of bind parameters,
  or a `SELECT` result shape that cannot decode into the requested entity.
  Any error diagnostic makes `mgen check-sql` exit with code 1.
- **uncovered** — the statement uses valid SQL constructs beyond the
  supported subset (e.g. `JOIN`). It cannot be statically checked, but it
  is not an error: the build still succeeds. Uncovered diagnostics are only
  printed with `-v`.

Diagnostics are printed as `file:line:col: severity: message`.

## What gets extracted

One universal rule applies to every guest language: a string literal is
treated as SQL when its content (after trimming leading whitespace) starts
a DML statement — `SELECT ..`, `INSERT INTO ..`, `UPDATE .. SET ..`,
`DELETE FROM ..` (case-insensitive, word-boundary matched). Everything else
is skipped, including:

- natural-language strings that merely start with a similar word
  (`"Insert user"`, `"Selects the row"`);
- lone keyword fragments produced by entity-generated dynamic-SQL builders
  (`"UPDATE "`, `" SET "`);
- `format!`-style templates containing `{` / `}` holes (their final text is
  built at runtime);
- doc comments and attribute literals (Rust), template strings
  (AssemblyScript), interpolated strings (C#).

### The dynamic-SQL boundary

Only *literals* are checked. SQL built at runtime — string concatenation,
`format!`, `String` variables, or the entity-generated `SQL_*` constants
referenced by name (`sql_stmt!(&Wallets::SQL_GET_BY_PK)`) — is not
re-resolved. The constant *definitions* themselves are literals and are
checked at their definition site, so generated entities stay covered;
composed runtime strings are the documented boundary of the analysis.

## Per-language usage

```bash
# Rust guest sources
mgen check-sql --lang rust --ddl sql/ddl.sql --input src/rust

# AssemblyScript guest sources
mgen check-sql --lang as --ddl sql/ddl.sql --input assembly

# C# guest sources
mgen check-sql --lang cs --ddl sql/ddl.sql --input src
```

`--input` may be a single file or a directory scanned recursively for the
language's extension (`.rs` / `.ts` / `.cs`). `--ddl` accepts several
files; their `CREATE TABLE` definitions are merged, and same-named tables
(deployment variants, e.g. a partitioned variant that adds the partition
key) get the union of their columns. `-v` additionally prints uncovered
diagnostics and a summary.

### Rust enhancements

- When a literal sits in a call that also receives a parameter list —
  `sql_params!(&(a, b))`, `&(a, b)`, `&(x)`, `&vec![..]` — the tuple arity
  is compared with the number of `?` placeholders. Parameters passed as
  variables or method results (`&params`, `&wallet.insert_params()`) are
  not statically known and skip this check.
- When the call has a turbofish entity (`mudu_query::<Wallets>`, or helpers
  like `query_one_entity::<Customer>`) and the literal is a `SELECT`, the
  select list is compared with the entity table's columns in DDL order
  (`Wallets` → `wallets`; `*` expands to the full column list). Scalar
  turbofish types (`mudu_query::<i64>`) map to no table and skip this
  check.
- Parameter tuple elements that are source literals (`42`, `0L`, `"s"`,
  `1.5`, `true`, `b".."`) are typed position-wise against the placeholder's
  target column (INSERT value position, UPDATE assignment, WHERE
  `col <op> ?`) using the compatibility table below. Non-literal elements
  and unmappable placeholders are skipped.

### Parameter type compatibility table

One table drives every parameter/column type check in the system — the
static `check-sql` literal check, the syscall-frame descriptor/value
consistency check, and the client-protocol pre-flight check. It is defined
in `mudu_binding::universal::uni_type_compat` and re-exported through
`sql_parser::check::param_type`:

| Parameter tag | Compatible column types |
| ------------- | ----------------------- |
| `null`        | every column |
| integer       | integer families (widening, e.g. i32 param into an i64 column) and `NUMERIC` |
| float         | `F32`, `F64`, `NUMERIC` |
| text          | `STRING`/`CHAR`, `DATE`/`TIME`/`TIMESTAMP`/`TIMESTAMP_TZ` (temporal parameters travel as text), `NUMERIC` (numeric parameters travel as plain strings; the final parse happens at the database) |
| blob          | `BLOB` |
| numeric       | `NUMERIC` |
| boolean       | nothing (the wire types have no boolean datum) |

Rationale: the table rejects the common authoring bug (a string literal or
value bound to an integer column) while accepting every representation the
wire protocols legitimately produce — numeric and temporal parameters are
string-shaped on the wire, and SQL NULL is type-agnostic. Everything the
table accepts is still validated by the database at execution; the checks
are an early, well-located error, not a full type system.

### AssemblyScript

All string literals are checked; bind arity is not associated in this
version (`params.bind(..)` sequences cannot be reliably tied to the SQL
literal).

### C#

When a literal is the second argument of `MuduSys.Query(session, sql, ...)`
or `MuduSys.Command(session, sql, ...)`, the number of arguments after the
SQL argument is compared with the number of `?` placeholders.

## Build integration

The example packages run `check-sql` as part of their default
`cargo make` chain, via `cargo run --manifest-path .../crates/tools/mudu_gen/Cargo.toml`
so no pre-installed `mgen` is required:

| Example        | When            | Command tail |
| -------------- | --------------- | ------------ |
| wallet         | after `generate` | `--lang rust --ddl sql/ddl.sql --input src/rust` |
| vote           | after `generate` | `--lang rust --ddl sql/ddl.sql sql/type.sql --input src/rust` |
| app1           | after `generate` | `--lang rust --ddl sql/ddl.sql sql/ddl_dynamic.sql --input src/rust` |
| tpcc           | after `generate` | `--lang rust --ddl sql/ddl.sql sql/ddl_warehouse_partitioned.sql sql/ddl_dynamic.sql --input src/rust` |
| wallet-as      | before `asc`     | `--lang as --ddl sql/ddl.sql --input assembly` |
| wallet-cs      | before `dotnet build` | `--lang cs --ddl sql/ddl.sql --input src` |

Some examples keep an extra `sql/ddl_dynamic.sql` file for tables that are
*not* created by the package DDL at install time but do exist when the
procedures run — created by the benchmark driver (tpcc's `seckill_*` /
`tpcc_hotspot`) or by a procedure itself (app1's `wallets`). The checker
reads these files together with the install-time DDL so statements against
those tables resolve their schema.

## Implementation map

- Check core (language-independent): `crates/common/sql_parser/src/check/`
  (`check_sql`, `count_params`, `select_result_shape`, `SqlDiagnostic`).
- Extractors and CLI driver: `crates/tools/mudu_gen/src/src_check/`.
- CLI wiring: `crates/tools/mudu_gen/tool/main.rs` (`mgen check-sql`).
