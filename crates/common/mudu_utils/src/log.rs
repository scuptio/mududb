use crate::init_log::{INIT, setup_with_console};

pub fn log_setup(level: &str) {
    INIT.call_once(|| {
        setup_with_console(level, "", false);
    });
}

pub fn log_setup_ex(level: &str, parse: &str, enable_console: bool) {
    INIT.call_once(|| {
        setup_with_console(level, parse, enable_console);
    });
}
