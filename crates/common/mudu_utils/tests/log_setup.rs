//! Runs `log_setup` in a fresh test process: subscriber initialization is
//! guarded by a process-wide `Once`, so the closure only executes for the
//! first caller in the process.

#[test]
fn log_setup_initializes_tracing() {
    mudu_utils::log::log_setup("info");
}
