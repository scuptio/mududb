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

fn init_level_console(
    level_filter:LevelFilter
) {
    let registry = tracing_subscriber::registry();
    let console_layer = console_subscriber::spawn();
    registry
        .with(console_layer)
        .with(my_tracing_subscriber!()
            .with_filter(level_filter)
        ).init();
}

fn init_level(
    level_filter:LevelFilter
) {
    let registry = tracing_subscriber::registry();
    registry
        .with(my_tracing_subscriber!()
            .with_filter(level_filter)
        ).init();
}

fn init_level_env_console(
    level_filter:LevelFilter,
    env_filter:EnvFilter,
) {
    let registry = tracing_subscriber::registry();
    let console_layer = console_subscriber::spawn();
    registry
        .with(console_layer)
        .with(my_tracing_subscriber!()
            .with_filter(level_filter)
            .with_filter(env_filter)
        ).init();
}

fn init_level_env(
    level_filter:LevelFilter,
    env_filter:EnvFilter,
) {
    let registry = tracing_subscriber::registry();
    registry
        .with(my_tracing_subscriber!()
            .with_filter(level_filter)
            .with_filter(env_filter)
        ).init();
}
fn _setup_with_console(level: &str, parse: &str, enable_console_layer: bool) {
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

    if !parse.is_empty()  {
        let env_filter = EnvFilter::builder()
            .with_default_directive(level_filter.into())
            .parse(parse)
            .unwrap();
        if enable_console_layer {
            init_level_env_console(level_filter, env_filter);
        } else {
            init_level_env(level_filter, env_filter);
        }
    } else {
        if enable_console_layer {
            init_level_console(level_filter)
        } else {
            init_level(level_filter)
        }
    };
}

