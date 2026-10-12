# index

B-tree index implementation and key encodings used by the execution layer.

## Role

`index` sits just above `crates/db-kernel/catalog` at the bottom of the
kernel dependency chain defined in
[ARCHITECTURE.md](../../../ARCHITECTURE.md). It depends only on the common
crates (`mudu`, `mudu_contract`, `mudu_sys`, `mudu_type`) and is consumed by
`crates/db-kernel/storage`, `crates/db-kernel/xcontract`, and
`crates/db-kernel/server`.

## Key modules

- `btree` — ordered map index over encoded key tuples.
- `index_key` — index key encoding and the thread-local comparator context.
