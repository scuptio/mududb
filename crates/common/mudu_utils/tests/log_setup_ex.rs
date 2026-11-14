//! Same fresh-process reasoning as `log_setup.rs`, for `log_setup_ex` with an
//! additional env-filter directive.

#[test]
fn log_setup_ex_initializes_tracing_with_filter() {
    mudu_utils::log::log_setup_ex("debug", "mudu=debug", false);
}
