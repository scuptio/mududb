//! Extra coverage tests for `rust::procedure`: validation arms, the
//! partitioned delegate wrappers, and the hotspot counter — paths the main
//! roundtrip tests in `procedure.rs` do not reach.

use crate::rust::procedure::{
    tpcc_delivery_partitioned, tpcc_hotspot_hit, tpcc_hotspot_hit_partitioned, tpcc_new_order,
    tpcc_new_order_partitioned, tpcc_order_status_partitioned, tpcc_payment, tpcc_seed,
    tpcc_seed_partitioned, tpcc_stock_level_partitioned,
};
use crate::test_lock;
use mududb::common::id::OID;
use mududb::contract::{sql_params, sql_stmt};
use mududb::sys::env_var::temp_dir;
use mududb::sys::time::system_time_now;
use mududb::sys_interface::sync_api::{mudu_batch, mudu_close, mudu_open};
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

fn temp_db_path(name: &str) -> PathBuf {
    let suffix = system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    temp_dir().join(format!("tpcc_extra_{name}_{suffix}.db"))
}

/// Opens a session on a fresh database with the plain TPC-C schema.
fn open_seeded_db(name: &str) -> OID {
    let db_path = temp_db_path(name);
    mudu_adapter::config::reset_db_path_override_for_test();
    mudu_adapter::syscall::set_db_path(&db_path);
    let xid = mudu_open().unwrap();
    let ddl = include_str!("../../sql/ddl.sql");
    mudu_batch(xid, sql_stmt!(&ddl), sql_params!(&())).unwrap();
    xid
}

#[test]
#[cfg_attr(miri, ignore)]
fn seed_validates_positive_counts() {
    let _guard = test_lock().lock().unwrap();
    // Validation fails before any database access, so a dummy xid suffices.
    let err = tpcc_seed(0, 0, 1, 1, 1, 1).unwrap_err();
    assert!(err.to_string().contains("warehouse_count must be positive"));
    let err = tpcc_seed(0, 1, 0, 1, 1, 1).unwrap_err();
    assert!(err.to_string().contains("district_count must be positive"));
    let err = tpcc_seed(0, 1, 1, 0, 1, 1).unwrap_err();
    assert!(err.to_string().contains("customer_count must be positive"));
    let err = tpcc_seed(0, 1, 1, 1, 0, 1).unwrap_err();
    assert!(err.to_string().contains("item_count must be positive"));
    let err = tpcc_seed(0, 1, 1, 1, 1, 0).unwrap_err();
    assert!(err.to_string().contains("initial_stock must be positive"));
    let err = tpcc_seed_partitioned(0, -1, 1, 1, 1, 1).unwrap_err();
    assert!(err.to_string().contains("warehouse_count must be positive"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn new_order_validates_request_shape() {
    let _guard = test_lock().lock().unwrap();
    let err = tpcc_new_order(0, 0, 1, 1, vec![1], vec![1], vec![1]).unwrap_err();
    assert!(err.to_string().contains("warehouse_id must be positive"));
    let err = tpcc_new_order(0, 1, 1, 1, vec![], vec![], vec![]).unwrap_err();
    assert!(err.to_string().contains("at least one item"));
    let err = tpcc_new_order(0, 1, 1, 1, vec![1], vec![1], vec![1, 2]).unwrap_err();
    assert!(err.to_string().contains("length mismatch"));

    // Partitioned new orders must be supplied from the local warehouse.
    let err = tpcc_new_order_partitioned(0, 1, 1, 1, vec![1], vec![2], vec![1]).unwrap_err();
    assert!(err.to_string().contains("local supplier warehouses"));
    // The same shape with a local supplier passes validation and then fails
    // on the missing schema (no database needed for the validation itself).
    let err = tpcc_new_order_partitioned(0, 1, 1, 1, vec![1], vec![1], vec![1]).unwrap_err();
    assert!(!err.to_string().contains("local supplier warehouses"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn payment_and_stock_level_validate_ids() {
    let _guard = test_lock().lock().unwrap();
    let err = tpcc_payment(0, 0, 1, 1, 1).unwrap_err();
    assert!(err.to_string().contains("warehouse_id must be positive"));
    let err = tpcc_stock_level_partitioned(0, 0, 1, 1).unwrap_err();
    assert!(err.to_string().contains("warehouse_id must be positive"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn partitioned_delegates_run_on_the_plain_schema() {
    let _guard = test_lock().lock().unwrap();
    let xid = open_seeded_db("partitioned_delegates");
    tpcc_seed(xid, 1, 1, 2, 2, 20).unwrap();
    let order = tpcc_new_order(xid, 1, 1, 1, vec![1, 2], vec![1, 1], vec![2, 1]).unwrap();
    assert!(order.contains("lines=2"));

    // The *_partitioned order-status / delivery / stock-level wrappers
    // delegate straight to the base procedures.
    assert_eq!(tpcc_order_status_partitioned(xid, 1, 1, 1).unwrap(), "NEW");
    assert!(
        tpcc_delivery_partitioned(xid, 1, 1, 9)
            .unwrap()
            .contains("carrier=9")
    );
    assert_eq!(
        tpcc_order_status_partitioned(xid, 1, 1, 1).unwrap(),
        "DELIVERED"
    );
    assert!(tpcc_stock_level_partitioned(xid, 1, 1, 20).unwrap() >= 0);
    mudu_close(xid).unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn hotspot_hit_counts_and_validates() {
    let _guard = test_lock().lock().unwrap();
    let xid = open_seeded_db("hotspot");
    mudu_batch(
        xid,
        sql_stmt!(
            &"CREATE TABLE tpcc_hotspot (h_w_id INTEGER NOT NULL, h_id INTEGER NOT NULL, h_counter INTEGER NOT NULL);
              INSERT INTO tpcc_hotspot (h_w_id, h_id, h_counter) VALUES (1, 1, 0)"
        ),
        sql_params!(&()),
    )
    .unwrap();

    assert_eq!(tpcc_hotspot_hit(xid, 1, 1).unwrap(), 1);
    assert_eq!(tpcc_hotspot_hit_partitioned(xid, 1, 1).unwrap(), 2);

    let err = tpcc_hotspot_hit(xid, 0, 1).unwrap_err();
    assert!(err.to_string().contains("warehouse_id must be positive"));
    let err = tpcc_hotspot_hit(xid, 1, 0).unwrap_err();
    assert!(err.to_string().contains("hot_id must be positive"));
    mudu_close(xid).unwrap();
}
