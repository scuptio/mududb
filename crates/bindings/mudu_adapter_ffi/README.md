# mudu_adapter_ffi

Versioned native adapter ABI for MuduDB, exported as a `cdylib`. All
buffers returned by this library are released by this library
(`mudu_adapter_free`).

## Role

FFI surface at the bottom of the SDK stack: the SDK runtime loads
`libmudu_adapter_ffi.so` (built and archived by
`script/sdk/adapter_runtime.py`) and drives a native MuduDB backend through
the exported ABI. The library dispatches `open` / `close` / `query` /
`command` / `batch` operations to `mudu_adapter` syscalls and
`sys_interface_standalone`, using `mudu_binding` codecs and `mudu_contract`
result types. It is consumed only as a dynamic library — never as a Rust
rlib — which is why it lives in its own isolated workspace (see
[../README.md](../README.md)).

## Key modules

- `src/lib.rs` — the whole ABI in a single file: `mudu_adapter_abi_version`,
  `mudu_adapter_call`, `mudu_adapter_free`, and the `AdapterBuffer` type.
- `include/mudu_adapter.h` — C header declaring the ABI v1 exports.
- `node/adapter.c` — Node-API bridge built into a separate `.node` addon.
