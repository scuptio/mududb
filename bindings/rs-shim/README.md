# MuduDB rs-shim

`mududb-rs-shim` exports the `mududb:component-shim/shim-api` WIT world.

This crate is for WebAssembly Component Model composition with non-Rust guest
languages such as AssemblyScript. It is not the native C ABI shim.

## Boundary

```text
AssemblyScript guest
  imports mududb:component-shim/system

mududb-rs-shim
  exports mududb:component-shim/system
  uses the Rust mududb facade crate internally
```

## Current State

The crate implements the component resources and value conversion surface:

- `types.value-*`
- `system.value-list`
- `system.sql-stmt`
- `system.result-set`
- `system.row`
- `open / close / query / command / batch`

`open` returns an object id represented internally through
`mududb::common::id::OID`. Query, command, and batch are forwarded through the
Rust `mududb` facade.
