use std::sync::Once;
use tracing::metadata::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, Registry};

static INIT: Once = Once::new();

static INIT_ENV: Once = Once::new();

#[cfg(not(target_arch = "wasm32"))]
fn register_console_layer(register: Registry, filter: LevelFilter) {
    let console_layer = console_subscriber::spawn();
    register.with(console_layer).with(
        tracing_subscriber::fmt::layer()
            .with_level(true)
            .with_ansi(false)
            .with_file(true)
            // display source code line numbers
            .with_line_number(true)
            .without_time()
            .with_filter(filter),
    )
        .init();
}

#[cfg(target_arch = "wasm32")]
fn register_console_layer(register: Registry, filter: LevelFilter) {
    register_layer(register, filter)
}

fn register_layer(register: Registry, filter: LevelFilter) {
    register.with(
        tracing_subscriber::fmt::layer()
            .with_level(true)
            .with_ansi(false)
            .with_file(true)
            // display source code line numbers
            .with_line_number(true)
            .without_time()
            .with_filter(filter),
    )
        .init();
}
fn _setup_with_console(level: &str, enable_console_layer: bool) {
    let filter = match level {
        "info" => { tracing_subscriber::filter::LevelFilter::INFO }
        "debug" => { tracing_subscriber::filter::LevelFilter::DEBUG }
        "trace" => { tracing_subscriber::filter::LevelFilter::TRACE }
        "warn" => { tracing_subscriber::filter::LevelFilter::WARN }
        "error" => { tracing_subscriber::filter::LevelFilter::ERROR }
        _ => { panic!("unknown level {}", level) }
    };
    let register = tracing_subscriber::registry();
    if enable_console_layer {
        register_console_layer(register, filter)
    } else {
        register_layer(register, filter)
    };
}


pub fn log_env_setup() {
    INIT_ENV.call_once(|| {
        env_logger::init();
    });
}


pub fn logger_setup(level: &str) {
    INIT.call_once(
        || { _setup_with_console(level, false); }
    );
}

pub fn logger_setup_with_console(level: &str) {
    INIT.call_once(
        || { _setup_with_console(level, true); }
    );
}