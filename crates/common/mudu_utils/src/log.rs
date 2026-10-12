use crate::init_log::{INIT, setup_with_console};

/// Initializes the global tracing subscriber at `level` without the
/// tokio-console layer.
///
/// Only the first call takes effect; later calls are no-ops. `level` must be
/// one of `trace`, `debug`, `info`, `warn`, `error`.
///
/// # Panics
///
/// Panics if `level` is not a recognized level name.
pub fn log_setup(level: &str) {
    INIT.call_once(|| {
        setup_with_console(level, "", false);
    });
}

/// Initializes the global tracing subscriber with an additional filter and an
/// optional tokio-console layer.
///
/// `parse` is a `tracing_subscriber::EnvFilter` directive string (e.g.
/// `"mudu=debug"`); when empty or invalid, filtering falls back to `level`
/// alone. `enable_console` spawns the `console-subscriber` layer. Only the
/// first call takes effect; later calls are no-ops.
///
/// # Panics
///
/// Panics if `level` is not one of `trace`, `debug`, `info`, `warn`, `error`.
pub fn log_setup_ex(level: &str, parse: &str, enable_console: bool) {
    INIT.call_once(|| {
        setup_with_console(level, parse, enable_console);
    });
}
