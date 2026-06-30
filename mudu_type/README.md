# mudu_type

`mudu_type` defines MuduDB's data type system: type identifiers, parameterized
type descriptors, in-memory values, conversion to and from textual, JSON,
MessagePack and binary representations, plus comparison and hashing hooks.
It sits on top of the primitive scalar definitions in `mudu::data_type` and
provides the typed value abstractions used by the language and contract layers.

## Responsibility

- Define the data type identifier enum and its per-type conversion/comparison
  function dispatch (`DatTypeID`).
- Represent parameterized type descriptors (`DatType`) and their serializable
  metadata (`DTInfo`).
- Model type parameters for numeric, string, temporal, array and record types
  (`DTPKind`, `DTPNumeric`, `DTPString`, `DTPTime`, `DTPTimestamp`, etc.).
- Provide a unified in-memory value container (`DatValue`) and a value/type pair
  (`DatTyped`).
- Define the `Datum` / `DatumDyn` traits for typed Rust values and map Rust
  types to their `DatType`.
- Implement input/output, send/receive, default and length function signatures
  for every supported type.
- Wrap external representations used for conversions (`DatBinary`, `DatTextual`,
  `DatMsgPack`).
- Expose convenience helpers for common conversions and scalar value
  constructors (`dt_function`, `ScalarType`, `array`, `record`).

## What does NOT belong here

- Primitive date/time/numeric scalar definitions live in `mudu::data_type`
  (`mudu`).
- SQL parsing and AST construction live in `sql_parser`.
- Client/server wire protocol, tuple encoding and session abstractions live in
  `mudu_contract`.
- Storage engine, page format and persistent I/O live in `mududb` and
  `mudu_kernel`.
- Query execution, Wasmtime runtime and package loading live in `mudu_runtime`.
- OS and async system abstractions live in `mudu_sys` and `mudu_sys_impl`.
- Language bindings, adapters and CLI surfaces live in `mudu_binding`,
  `mudu_adapter` and `mudu_cli`.
- Code generation from DDL/WIT lives in `mudu_gen`.

## Main public entry points

- `mudu_type::dat_type_id` — `DatTypeID` and per-type function dispatch.
- `mudu_type::dat_type` — `DatType` descriptor with optional parameters.
- `mudu_type::dat_value` — `DatValue`, the unified in-memory value container.
- `mudu_type::datum` — `Datum` / `DatumDyn` traits for typed values.
- `mudu_type::dat_typed` — `DatTyped`, a value paired with its `DatType`.
- `mudu_type::scalar_type` — `ScalarType`, a restricted scalar-only wrapper.
- `mudu_type::dt_info` — `DTInfo`, serializable type metadata.
- `mudu_type::type_error` — `TyErr` / `TyEC`, type-system error types.
- `mudu_type::dt_function` — convenience conversion helpers (textual, binary,
  JSON, MessagePack).
- `mudu_type::dt_fn_convert` / `dt_fn_compare` / `dt_fn_param` — function
  signatures for conversion, comparison and parameter parsing.
- `mudu_type::array` / `mudu_type::record` — constructors for array and record
  types.
- `mudu_type::dtp_numeric` / `dtp_string` / `dtp_time` / `dtp_timestamp` /
  `dtp_timestamptz` / `dtp_array` / `dtp_object` — concrete parameter types.
- `mudu_type::dat_binary` / `dat_textual` / `dat_msg_pack` — representation
  wrappers used by conversion functions.
