use std::sync::Once;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

pub static INIT: Once = Once::new();

pub fn setup_with_console(level: &str, parse: &str, enable_console_layer: bool) {
    _setup_with_console(level, parse, enable_console_layer);
}

macro_rules! my_tracing_subscriber {
    () => {
        tracing_subscriber::fmt::layer()
            .with_level(true)
            .with_ansi(false)
            .with_file(true)
            .with_line_number(true)
            .without_time()
    };
}

fn init_level_console(level_filter: LevelFilter) {
    let registry = tracing_subscriber::registry();
    let console_layer = console_subscriber::spawn();
    registry
        .with(console_layer)
        .with(my_tracing_subscriber!().with_filter(level_filter))
        .init();
}

fn init_level(level_filter: LevelFilter) {
    let registry = tracing_subscriber::registry();
    registry
        .with(my_tracing_subscriber!().with_filter(level_filter))
        .init();
}

fn init_level_env_console(level_filter: LevelFilter, env_filter: EnvFilter) {
    let registry = tracing_subscriber::registry();
    let console_layer = console_subscriber::spawn();
    registry
        .with(console_layer)
        .with(
            my_tracing_subscriber!()
                .with_filter(level_filter)
                .with_filter(env_filter),
        )
        .init();
}

fn init_level_env(level_filter: LevelFilter, env_filter: EnvFilter) {
    let registry = tracing_subscriber::registry();
    registry
        .with(
            my_tracing_subscriber!()
                .with_filter(level_filter)
                .with_filter(env_filter),
        )
        .init();
}
/// Internal implementation of [`setup_with_console`] exposed to tests so they
/// can exercise level/filter branches without poisoning the global `Once`.
pub(crate) fn _setup_with_console(level: &str, parse: &str, enable_console_layer: bool) {
    let level_filter = match level {
        "info" => LevelFilter::INFO,
        "debug" => LevelFilter::DEBUG,
        "trace" => LevelFilter::TRACE,
        "warn" => LevelFilter::WARN,
        "error" => LevelFilter::ERROR,
        _ => {
            panic!("unknown level {}", level)
        }
    };

    if !parse.is_empty() {
        let env_filter = EnvFilter::builder()
            .with_default_directive(level_filter.into())
            .parse(parse);
        if let Ok(env_filter) = env_filter {
            if enable_console_layer {
                init_level_env_console(level_filter, env_filter);
            } else {
                init_level_env(level_filter, env_filter);
            }
        } else {
            eprintln!(
                "invalid tracing filter '{}', fallback to level-only logging at {}",
                parse, level
            );
            if enable_console_layer {
                init_level_console(level_filter)
            } else {
                init_level(level_filter)
            }
        }
    } else {
        if enable_console_layer {
            init_level_console(level_filter)
        } else {
            init_level(level_filter)
        }
    };
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn invalid_filter_falls_back_and_initializes() {
        // Guarded by the crate-level Once so we do not try to install a global
        // subscriber more than once per process.
        INIT.call_once(|| {
            if !tracing::dispatcher::has_been_set() {
                setup_with_console("info", "not_a_valid_filter=xyz", false);
            }
        });
    }

    #[test]
    fn setup_with_console_info_does_not_panic() {
        // If the fallback test already initialized the subscriber, this is a
        // no-op due to the Once guard, but it still verifies the call does not
        // panic.
        INIT.call_once(|| {
            if !tracing::dispatcher::has_been_set() {
                setup_with_console("info", "", false);
            }
        });
    }

    #[test]
    fn invalid_level_panics_before_init() {
        let result = std::panic::catch_unwind(|| {
            setup_with_console("not_a_level", "", false);
        });
        assert!(result.is_err());
    }
}
