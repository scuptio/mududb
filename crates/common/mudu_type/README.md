# mudu_type

`mudu_type` defines MuduDB's data type system: type identifiers, parameterized
type descriptors, in-memory values, conversion to and from textual, JSON,
MessagePack and binary representations, plus comparison and hashing hooks.
It sits on top of the primitive scalar definitions in `mudu::data_type` and
provides the typed value abstractions used by the language and contract layers.

## Responsibility

- Define the type family enum and its per-type conversion/comparison
  function dispatch (`TypeFamily`).
- Represent parameterized type descriptors (`DataType`) and their serializable
  metadata (`DataTypeInfo`).
- Model type parameters for numeric, string, temporal, array and record types
  (`DataTypeParamKind`, `DataTypeParamNumeric`, `DataTypeParamString`, `DataTypeParamTime`, `DataTypeParamTimestamp`, etc.).
- Provide a unified in-memory value container (`DataValue`) and a value/type pair
  (`DataTyped`).
- Define the `Datum` / `DatumDyn` traits for typed Rust values and map Rust
  types to their `DataType`.
- Implement input/output, send/receive, default and length function signatures
  for every supported type.
- Wrap external representations used for conversions (`DataBinary`, `DataTextual`,
  `DataMsgPack`).
- Expose convenience helpers for common conversions and scalar value
  constructors (`data_type_function`, `ScalarType`, `array`, `record`).

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

- `mudu_type::type_family` — `TypeFamily` and per-type function dispatch.
- `mudu_type::data_type` — `DataType` descriptor with optional parameters.
- `mudu_type::data_value` — `DataValue`, the unified in-memory value container.
- `mudu_type::datum` — `Datum` / `DatumDyn` traits for typed values.
- `mudu_type::data_typed` — `DataTyped`, a value paired with its `DataType`.
- `mudu_type::scalar_type` — `ScalarType`, a restricted scalar-only wrapper.
- `mudu_type::data_type_info` — `DataTypeInfo`, serializable type metadata.
- `mudu_type::type_error` — `TyErr` / `TyEC`, type-system error types.
- `mudu_type::data_type_function` — convenience conversion helpers (textual, binary,
  JSON, MessagePack).
- `mudu_type::data_type_fn_convert` / `data_type_fn_compare` / `data_type_fn_param` — function
  signatures for conversion, comparison and parameter parsing.
- `mudu_type::array` / `mudu_type::record` — constructors for array and record
  types.
- `mudu_type::data_type_param_numeric` / `data_type_param_string` / `data_type_param_time` / `data_type_param_timestamp` /
  `data_type_param_timestamptz` / `data_type_param_array` / `data_type_param_record` — concrete parameter types.
- `mudu_type::data_binary` / `data_textual` / `data_msg_pack` — representation
  wrappers used by conversion functions.
