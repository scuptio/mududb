//! Excluded from the deterministic-simulation backend (`-F testing/ds`):
//! wasmtime `.mpk` execution-path tests over a full kernel server with real
//! OS sockets (see the sibling files' gate comments). Runs on the native
//! backend only.
#![cfg(not(feature = "ds"))]
#![cfg(target_os = "linux")]

#[path = "linux/py_spike_mpk.rs"]
mod py_spike_mpk;
