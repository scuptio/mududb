# sql_parser

`sql_parser` turns SQL source text into a typed AST for MuduDB. It is built on
`tree-sitter-sql` so the grammar is maintained separately, while this crate
focuses on extracting and validating the AST nodes that MuduDB cares about.

## Responsibility

- Parse DDL statements such as `CREATE TABLE`, partition rules and partition
  placements.
- Parse DML statements: `SELECT`, `INSERT`, `UPDATE` and `DELETE`.
- Parse utility statements including `COPY FROM/TO`, `DROP TABLE`, etc.
- Expose typed AST nodes and helper functions for binding/planning
  (`ast`, `parser`).
- Re-export generated tree-sitter constants for node kinds and field names
  (`ts_const`).

## What does NOT belong here

- Semantic analysis, name resolution or type checking — those belong to
  `mudu_kernel::sql`.
- Execution plans or storage concerns — those belong to `mudu_kernel`.
- Shared session/connection/value types — those belong to `mudu_contract`.

## Main public entry points

- `sql_parser::ast` — the typed SQL AST.
- `sql_parser::parser` — parsing entry points and helper functions.
- `sql_parser::ts_const` — generated tree-sitter node kind/field constants.
