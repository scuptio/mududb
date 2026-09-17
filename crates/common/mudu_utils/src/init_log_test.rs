//! Tests for the logging initialization helpers.
#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::init_log::{INIT, setup_with_console};

#[test]
fn setup_with_console_initializes_trace_level() {
    INIT.call_once(|| {
        if !tracing::dispatcher::has_been_set() {
            setup_with_console("trace", "", false);
        }
    });
}

#[test]
fn setup_with_console_falls_back_on_invalid_filter() {
    INIT.call_once(|| {
        if !tracing::dispatcher::has_been_set() {
            setup_with_console("info", "not_a_valid_filter=xyz", false);
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
fn setup_with_console_respects_parse_string() {
    INIT.call_once(|| {
        if !tracing::dispatcher::has_been_set() {
            setup_with_console("debug", "mudu_utils=info", false);
        }
    });
}

#[test]
fn setup_with_console_can_enable_console_layer() {
    INIT.call_once(|| {
        if !tracing::dispatcher::has_been_set() {
            setup_with_console("info", "", true);
        }
    });
}
