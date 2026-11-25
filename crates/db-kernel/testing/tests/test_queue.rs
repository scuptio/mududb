//! Excluded from the deterministic-simulation backend (`-F testing/ds`):
//! the queue observability endpoint is served by the actix management HTTP
//! service, which the simulation build does not start, and the crash
//! recovery case spawns a real `mudud` subprocess killed with SIGKILL. Runs
//! on the native Linux backend only.
#![cfg(not(feature = "ds"))]
#![cfg(target_os = "linux")]

#[path = "linux/test_queue.rs"]
mod test_queue;
