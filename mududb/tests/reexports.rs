//! Smoke test for `mududb` public re-exports and macros.
#![allow(clippy::unwrap_used)]

use mududb::{common, error, mudu, sql_params, sql_stmt};

/// Smoke test that verifies the core re-exports and macros are reachable.
#[test]
fn re_exports_and_macros_are_reachable() {
    let _ = mudu::common::id::OID::default();
    let _ = common::id::OID::default();
    let _ = error::ErrorCode::NotFound;
    let stmt = sql_stmt!("SELECT 1");
    let params = sql_params!(&(1i32,));
    assert_eq!(stmt, "SELECT 1");
    assert_eq!(params, &(1i32,));
}
