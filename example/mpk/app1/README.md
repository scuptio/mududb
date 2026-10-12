# app1 Example (runtime test package)

Test procedure package (`app1`) for the `runtime_wasm` component/procedure
tests. This crate rebuilds the prebuilt
[`runtime_wasm/data/wasm_module/app1.mpk`](../../../crates/db-kernel/runtime_wasm/data/wasm_module/app1.mpk)
guest package from source: three trivial procedures (`proc_mtp`,
`proc2_mtp`, `proc_sys_call_mtp`) matching the historical package descriptor
in `package/package.desc.json`.

## Procedures

| Procedure | Description |
|-----------|-------------|
| `proc_mtp` | Echoes the scalar arguments: returns `(a + b, "xid:.., a=.., b=.., c=..")`. |
| `proc2_mtp` | Same echo behavior as `proc_mtp`, declared as a second procedure. |
| `proc_sys_call_mtp` | Exercises real host syscalls: creates the `wallets` table, seeds two rows, and returns the query results rendered as text. |

## Build the `.mpk` package

From this directory:

```bash
cargo make package
```

This runs `mgen entity` plus `mgen check-sql` (against `sql/ddl.sql` and
`sql/ddl_dynamic.sql`, the latter describing the table the procedure itself
creates at runtime), `mtp` transpilation, a `wasm32-wasip2` release build,
and `mpm-build create`. The cargo package is named `mod_0`, so the output
is `target/wasm32-wasip2/release/mod_0.mpk` under the repository-root
`target/`.

## Redeploying the prebuilt package

The runtime tests load the checked-in copy, not the freshly built one.
After rebuilding, redeploy from the repository root with:

```bash
cp target/wasm32-wasip2/release/mod_0.mpk \
   crates/db-kernel/runtime_wasm/data/wasm_module/app1.mpk
```

Rebuild and redeploy whenever the guest/host wire format changes.

## Files of interest

- `src/rust/procedures.rs` — the handwritten Rust procedures.
- `src/generated/` — the transpiler-generated async wrappers.
- `sql/ddl.sql` / `sql/ddl_dynamic.sql` — install-time and runtime-created schemas.
- `package/package.desc.json` — the historical package descriptor.
- `Makefile.toml` — the build pipeline.
