//! Tests for the logging initialization helpers.
#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::init_log::{_setup_with_console, INIT, setup_with_console};

#[test]
fn setup_with_console_initializes_once_per_process() {
    // The global OnceLock runs this closure exactly once per test process;
    // sibling branches of the level/filter/console matrix are covered by the
    // catch_unwind tests below (they run against the already-set global
    // dispatcher and exercise `_setup_with_console` directly).
    INIT.call_once(|| {
        if !tracing::dispatcher::has_been_set() {
            setup_with_console("debug", "mudu_utils=info", true);
        }
    });
}

#[test]
fn setup_with_console_panics_on_invalid_level() {
    let result = std::panic::catch_unwind(|| {
        setup_with_console("not_a_level", "", false);
    });
    assert!(result.is_err());
}

#[test]
fn setup_branches_run_before_the_global_init_conflict() {
    // Ensure a global subscriber is installed; every `_setup_with_console`
    // call below then executes its level/filter/console branch and panics
    // inside tracing's `.init()` because the global default already exists.
    INIT.call_once(|| {
        if !tracing::dispatcher::has_been_set() {
            setup_with_console("info", "", false);
        }
    });

    for level in ["debug", "trace", "warn", "error"] {
        let result = std::panic::catch_unwind(|| _setup_with_console(level, "", false));
        assert!(result.is_err());
    }
    let with_filter = std::panic::catch_unwind(|| _setup_with_console("info", "mudu=debug", false));
    assert!(with_filter.is_err());

    // The console-subscriber layer needs a tokio runtime to spawn into.
    let runtime = crate::task_async::build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        let console = std::panic::catch_unwind(|| _setup_with_console("info", "", true));
        assert!(console.is_err());
        let console_env =
            std::panic::catch_unwind(|| _setup_with_console("warn", "mudu=trace", true));
        assert!(console_env.is_err());
    });
}
