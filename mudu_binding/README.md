# mudu_binding

Bridge between `mudu` core types and portable, serializable representations used for FFI, RPC and storage. This crate defines the universal data/procedure/system types, record and field descriptors, and the codecs that translate them to and from the internal `mudu` types.

## Responsibility

- Portable `universal` types for values, rows, OIDs, procedure parameters and results, commands, queries, sessions, SQL statements and errors.
- Record and field definitions that map universal record types to `mudu` tuple descriptors.
- Procedure invocation wrappers for sync and async call paths.
- Command and query invocation serialization helpers for the system interface.
- Codecs and adapters that serialize/deserialize between `mudu` core types and universal byte/JSON representations.

## What does NOT belong here

- Core database engine, storage or transaction logic: lives in `mududb`, `mudu`, and `mudu_kernel`.
- Primitive type system definitions: lives in `mudu_type`.
- Trait contracts between subsystems: lives in `mudu_contract`.
- Runtime, host sandboxing, and language-specific guest/host binding generation: lives in `mudu_runtime`, `mudu_sys`, `mudu_sys_wasm`, `mudu_wasm`, and `bindings`.

## Main public entry points

- `mudu_binding::codec` — serialization codecs and adapters; includes `SqlParamPair`.
- `mudu_binding::procedure` — `procedure_invoke` wrappers.
- `mudu_binding::record` — `field_def` and `record_def`.
- `mudu_binding::system` — `command_invoke` and `query_invoke`.
- `mudu_binding::universal` — portable types for data, procedures, commands, queries, sessions, results and errors.
