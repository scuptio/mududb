//! Excluded from the deterministic-simulation backend (`-F testing/ds`):
//! wasmtime `.mpk` execution-path tests over a full kernel server with real
//! OS sockets (see the sibling files' gate comments). Runs on the native
//! backend only.
#![cfg(not(feature = "ds"))]
#![cfg(target_os = "linux")]

#[path = "linux/test_tpcc_concurrent_procedure.rs"]
mod test_tpcc_concurrent_procedure;
