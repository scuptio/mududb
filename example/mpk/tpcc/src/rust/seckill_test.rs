//! Tests for the seckill (flash-sale) procedures against the standalone
//! adapter: seeding, the hot-row buy path, sold-out behavior, and input
//! validation.

use crate::rust::seckill::{
    seckill_buy, seckill_buy_partitioned, seckill_seed, seckill_seed_partitioned,
};
use crate::test_lock;
use mududb::common::id::OID;
use mududb::contract::{sql_params, sql_stmt};
use mududb::sys::env_var::temp_dir;
use mududb::sys::time::system_time_now;
use mududb::sys_interface::sync_api::{mudu_batch, mudu_close, mudu_open, mudu_query};
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

fn open_db(name: &str) -> OID {
    let suffix = system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let db_path: PathBuf = temp_dir().join(format!("tpcc_seckill_{name}_{suffix}.db"));
    mudu_adapter::config::reset_db_path_override_for_test();
    mudu_adapter::syscall::set_db_path(&db_path);
    let xid = mudu_open().unwrap();
    mudu_batch(
        xid,
        sql_stmt!(
            &"CREATE TABLE seckill_item (si_id INTEGER PRIMARY KEY, si_name TEXT NOT NULL, si_stock INTEGER NOT NULL, si_sold INTEGER NOT NULL, si_price INTEGER NOT NULL);
              CREATE TABLE seckill_order (so_item_id INTEGER NOT NULL, so_id INTEGER NOT NULL, so_user_id INTEGER NOT NULL, so_amount INTEGER NOT NULL, so_payload TEXT NOT NULL, PRIMARY KEY (so_item_id, so_id))"
        ),
        sql_params!(&()),
    )
    .unwrap();
    xid
}

fn stock_of(xid: OID, item_id: i32) -> Option<i32> {
    mudu_query::<i32>(
        xid,
        sql_stmt!(&"SELECT si_stock FROM seckill_item WHERE si_id = ?"),
        sql_params!(&(item_id,)),
    )
    .unwrap()
    .next_record()
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn seckill_seed_and_buy_until_sold_out() {
    let _guard = test_lock().lock().unwrap();
    let xid = open_db("sold_out");
    seckill_seed(xid, 2, 1).unwrap();
    assert_eq!(stock_of(xid, 1), Some(1));
    assert_eq!(stock_of(xid, 2), Some(1));

    // First buy succeeds and decrements the hot row; the second is sold out.
    let verdict = seckill_buy(xid, 1, 1, 100, 100, "payload".to_string()).unwrap();
    assert_eq!(verdict, "ok");
    assert_eq!(stock_of(xid, 1), Some(0));
    let verdict = seckill_buy_partitioned(xid, 1, 2, 100, 100, "payload".to_string()).unwrap();
    assert_eq!(verdict, "sold_out");
    assert_eq!(stock_of(xid, 1), Some(0));

    // A missing item is also sold out.
    let verdict = seckill_buy(xid, 99, 3, 100, 100, "payload".to_string()).unwrap();
    assert_eq!(verdict, "sold_out");
    mudu_close(xid).unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn seckill_partitioned_wrappers_delegate() {
    let _guard = test_lock().lock().unwrap();
    let xid = open_db("partitioned");
    seckill_seed_partitioned(xid, 1, 5).unwrap();
    assert_eq!(stock_of(xid, 1), Some(5));
    let verdict = seckill_buy_partitioned(xid, 1, 1, 100, 100, "payload".to_string()).unwrap();
    assert_eq!(verdict, "ok");
    assert_eq!(stock_of(xid, 1), Some(4));
    mudu_close(xid).unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn seckill_validates_inputs() {
    let _guard = test_lock().lock().unwrap();
    // Validation fails before any database access, so a dummy xid suffices.
    let err = seckill_seed(0, 0, 1).unwrap_err();
    assert!(err.to_string().contains("item_count must be positive"));
    let err = seckill_seed(0, 1, -1).unwrap_err();
    assert!(err.to_string().contains("initial_stock must be positive"));
    let err = seckill_buy(0, 0, 1, 1, 1, "p".to_string()).unwrap_err();
    assert!(err.to_string().contains("item_id must be positive"));
    let err = seckill_buy(0, 1, 0, 1, 1, "p".to_string()).unwrap_err();
    assert!(err.to_string().contains("order_id must be positive"));
    let err = seckill_buy(0, 1, 1, 0, 1, "p".to_string()).unwrap_err();
    assert!(err.to_string().contains("user_id must be positive"));
    let err = seckill_buy(0, 1, 1, 1, 0, "p".to_string()).unwrap_err();
    assert!(err.to_string().contains("amount must be positive"));
}
