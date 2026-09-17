# wallet-py

The wallet example guest written in **Python**, compiled to a WASM component
with [componentize-py](https://github.com/bytecodealliance/componentize-py)
and installed into mudud as `wallet-py.mpk`. It mirrors
[wallet-as](../wallet-as) procedure-for-procedure (`create_user`, `deposit`,
`withdraw`, `transfer_funds`, `balance`) and is the reference for the Python
guest toolchain. `update_profile` demonstrates a **user-defined record
type** as a procedure parameter and return type.

## How it works

```
procedures.py  ──mtp(python)──►  generated/procedures_gen.py  (mp2-* adapter)
                              +  generated/procedures_gen.wit (world: import
                                 mududb:api/system + mp2-<kebab> exports)
                              +  package/package.desc.json
wit/types.wit  ──mgen message──► gentypes.py (Profile/Address dataclasses +
                                 xxx_to_value/xxx_from_value codecs)
componentize-py ──► build/wallet_py.wasm ──► wasm-tools validate
mpm-build ──► target/py-guest/wallet-py.mpk
```

- `procedures.py` — business logic written against the canonical Python
  facade (`mududb.db.Database`, `mududb.sql.{SqlStmt,Params}`,
  `mududb.result.as_i64`). Each `# mudu-proc` function's first parameter is
  the bound session `UniOid` (injected by the adapter from
  `UniProcedureParam.session`); the remaining parameters arrive as plain
  Python values.
- `wit/types.wit` — the project record types (`address`, `profile`). mgen
  generates them into `gentypes.py` (a single self-contained module);
  `mtp --type-wit wit/types.wit --type-import gentypes` lets procedure
  signatures use them. On the procedure byte pipe a record travels as the
  positional `uni-data-value` record case (declaration order, names dropped
  by the host); the mtp adapter decodes it through the binding record
  bridge (`mududb.codec.bridge.record_field_values`) composed with
  `gentypes.profile_from_value`, and encodes the returned record
  symmetrically. A WIT `bool` field crosses the JSON/desc surface as i32
  0/1 (the host has no boolean data-type family).
- The mtp-generated adapter (`generated/procedures_gen.py`) wires the
  componentize-py `wit_world.imports.system` byte pipe into
  `mududb.sys.set_transport` at import time, so facade calls reach the host.
- The `mududb` package (`crates/sdk/bindings/python`) is bundled into the
  component; its generated codecs frame MSSP in-guest.

## Build

Requirements: `componentize-py` 0.25.1 (`pipx install
componentize-py==0.25.1`), `wasm-tools`, `zip`, and the Rust toolchain.

```sh
cargo make package    # produces target/py-guest/wallet-py.mpk
```

## Test

```sh
# end-to-end (builds a real mudud backend; skips when the mpk is absent):
cd ../../../db-kernel   # crates/db-kernel
cargo test -p testing --test wallet_py_mpk

# the whole suite incl. rebuild:
cargo make test       # in crates/db-kernel/testing
```

## Notes

- Component size is ~6–19 MB and first instantiation is dominated by the
  CPython cold start (tens of seconds in e2e) — the inherent cost of the
  interpreter-in-WASM route; fine for install-once, invoke-many procedures.
- The Python facade is operation-aligned with the AssemblyScript, C#, and
  Rust guests; see `doc/dev/binding_api_surface.md` for the canonical
  surface and the behavioral consistency scenario.
