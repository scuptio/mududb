use std::sync::Once;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

pub static INIT: Once = Once::new();

pub fn setup_with_console(level: &str, parse: &str, enable_console_layer: bool) {
    _setup_with_console(level, parse, enable_console_layer);
}

fn _setup_with_console(level: &str, _parse: &str, enable_console_layer: bool) {
    let level_filter = match level {
        "info" => tracing_subscriber::filter::LevelFilter::INFO,
        "debug" => tracing_subscriber::filter::LevelFilter::DEBUG,
        "trace" => tracing_subscriber::filter::LevelFilter::TRACE,
        "warn" => tracing_subscriber::filter::LevelFilter::WARN,
        "error" => tracing_subscriber::filter::LevelFilter::ERROR,
        _ => {
            panic!("unknown level {}", level)
        }
    };
    /*
    let env_filter = EnvFilter::builder()
        .with_default_directive(level_filter.into()) // 默认级别
        .parse(parse) // 只显示当前 crate 的 debug 日志
        .unwrap();
    */
    let register = tracing_subscriber::registry();

    if enable_console_layer {
        let console_layer = console_subscriber::spawn();
        register
            .with(console_layer)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_level(true)
                    .with_ansi(false)
                    .with_file(true)
                    // display source code line numbers
                    .with_line_number(true)
                    .without_time()
                    .with_filter(level_filter)
            )
            .init();
    } else {
        register
            .with(
                tracing_subscriber::fmt::layer()
                    .with_level(true)
                    .with_ansi(false)
                    .with_file(true)
                    // display source code line numbers
                    .with_line_number(true)
                    .without_time()
                    .with_filter(level_filter)
            )
            .init();
    };
}
