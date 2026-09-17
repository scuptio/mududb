//! Regression tests for `ServerCfg` page-size validation.
#![allow(clippy::unwrap_used)]

use mudu_kernel::server::routing::RoutingMode;
use mudu_kernel::server::server_cfg::ServerCfg;
use mudu_kernel::storage::page::page_block_ref::DEFAULT_PAGE_SIZE;

fn test_cfg() -> ServerCfg {
    ServerCfg::new(
        1,
        "127.0.0.1".to_string(),
        0,
        "/tmp/mudu_test_data".to_string(),
        "/tmp/mudu_test_log".to_string(),
        RoutingMode::ConnectionId,
    )
    .unwrap()
}

#[test]
fn default_page_size_is_4k() {
    let cfg = test_cfg();
    assert_eq!(cfg.page_size(), DEFAULT_PAGE_SIZE);
}

#[test]
fn with_page_size_accepts_power_of_two() {
    let cfg = test_cfg().with_page_size(8192).unwrap();
    assert_eq!(cfg.page_size(), 8192);
}

#[test]
fn with_page_size_rejects_non_power_of_two() {
    let err = test_cfg().with_page_size(5000).unwrap_err();
    assert!(err.to_string().contains("not a power of two"));
}

#[test]
fn with_page_size_rejects_below_default() {
    let err = test_cfg().with_page_size(2048).unwrap_err();
    assert!(err.to_string().contains("below minimum"));
}
