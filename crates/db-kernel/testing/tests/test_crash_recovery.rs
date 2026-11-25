//! Excluded from the deterministic-simulation backend (`-F testing/ds`):
//! spawns a real `mudud` subprocess and kills it with SIGKILL, exercising
//! power-loss recovery semantics the simulator cannot provide. Runs on the
//! native Linux backend only.
#![cfg(not(feature = "ds"))]
#![cfg(feature = "socket")]
#![cfg(target_os = "linux")]

#[path = "linux/test_crash_recovery.rs"]
mod test_crash_recovery;
