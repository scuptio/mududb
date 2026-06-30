use crate::rust::procedures;
use crate::rust::wallets::object::Wallets;
use mududb::common::id::OID;
use mududb::contract::database::entity_set::RecordSet;
use mududb::contract::{sql_params, sql_stmt};
use mududb::sys::sync::SMutex;
use mududb::sys_interface::sync_api::{mudu_batch, mudu_close, mudu_open, mudu_query};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

// These integration tests open a real SQLite-backed database via rusqlite,
// which calls native SQLite FFI functions. Miri cannot execute those foreign
// functions, so skip the whole suite under Miri.

static TEST_MUTEX: OnceLock<SMutex<()>> = OnceLock::new();

#[test]
#[cfg_attr(miri, ignore)]
fn create_update_and_delete_user() {
    let _guard = test_mutex().lock().unwrap();
    let mut db = TestDb::new();
    let xid = db.open_session();

    procedures::create_user(xid, 3, "Carol".to_string(), "carol@example.com".to_string()).unwrap();

    assert_eq!(
        db.query_count("SELECT COUNT(*) FROM users WHERE user_id = ?", &(3,)),
        1
    );
    assert_eq!(
        db.query_string("SELECT name FROM users WHERE user_id = ?", &(3,)),
        Some("Carol".to_string())
    );
    assert_eq!(
        db.query_string("SELECT email FROM users WHERE user_id = ?", &(3,)),
        Some("carol@example.com".to_string())
    );
    assert_eq!(db.query_wallet(3).unwrap().get_balance(), &Some(0));

    procedures::update_user(xid, 3, "Caroline".to_string(), "".to_string()).unwrap();
    assert_eq!(
        db.query_string("SELECT name FROM users WHERE user_id = ?", &(3,)),
        Some("Caroline".to_string())
    );
    assert_eq!(
        db.query_string("SELECT email FROM users WHERE user_id = ?", &(3,)),
        Some("carol@example.com".to_string())
    );

    procedures::delete_user(xid, 3).unwrap();
    assert_eq!(
        db.query_count("SELECT COUNT(*) FROM users WHERE user_id = ?", &(3,)),
        0
    );
    assert!(db.query_wallet(3).is_none());
}

#[test]
#[cfg_attr(miri, ignore)]
fn delete_user_rejects_non_zero_balance() {
    let _guard = test_mutex().lock().unwrap();
    let mut db = TestDb::new();
    let xid = db.open_session();

    let err = procedures::delete_user(xid, 1).unwrap_err();
    assert!(
        err.message().contains("non-zero balance"),
        "unexpected error: {err:?}"
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn transfer_funds_moves_balance_and_writes_transaction() {
    let _guard = test_mutex().lock().unwrap();
    let mut db = TestDb::new();
    let xid = db.open_session();

    procedures::transfer_funds(xid, 1, 2, 500).unwrap();

    assert_eq!(db.query_wallet(1).unwrap().get_balance(), &Some(9500));
    assert_eq!(db.query_wallet(2).unwrap().get_balance(), &Some(10500));
    assert_eq!(
        db.query_count(
            "SELECT COUNT(*) FROM transactions WHERE from_user = ? AND to_user = ? AND amount = ?",
            &(1, 2, 500),
        ),
        1
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn transfer_rejects_self_transfer() {
    let _guard = test_mutex().lock().unwrap();
    let mut db = TestDb::new();
    let xid = db.open_session();

    let err = procedures::transfer(xid, 1, 1, 100).unwrap_err();
    assert!(err.message().contains("self"), "unexpected error: {err:?}");
}

#[test]
#[cfg_attr(miri, ignore)]
fn deposit_withdraw_and_purchase_update_balance_and_transactions() {
    let _guard = test_mutex().lock().unwrap();
    let mut db = TestDb::new();
    let xid = db.open_session();

    procedures::deposit(xid, 1, 250).unwrap();
    procedures::withdraw(xid, 1, 100).unwrap();
    procedures::purchase(xid, 1, 50, "book".to_string()).unwrap();

    assert_eq!(db.query_wallet(1).unwrap().get_balance(), &Some(10100));
    assert_eq!(
        db.query_count(
            "SELECT COUNT(*) FROM transactions WHERE trans_type = ?",
            &(String::from("DEPOSIT"),),
        ),
        1
    );
    assert_eq!(
        db.query_count(
            "SELECT COUNT(*) FROM transactions WHERE trans_type = ?",
            &(String::from("WITHDRAW"),),
        ),
        1
    );
    assert_eq!(
        db.query_count(
            "SELECT COUNT(*) FROM transactions WHERE trans_type = ?",
            &(String::from("PURCHASE"),),
        ),
        1
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn withdraw_rejects_insufficient_funds() {
    let _guard = test_mutex().lock().unwrap();
    let mut db = TestDb::new();
    let xid = db.open_session();

    let err = procedures::withdraw(xid, 1, 20000).unwrap_err();
    assert!(
        err.message().contains("Insufficient funds"),
        "unexpected error: {err:?}"
    );
}

fn test_mutex() -> &'static SMutex<()> {
    TEST_MUTEX.get_or_init(|| SMutex::new(()))
}

struct TestDb {
    path: PathBuf,
    session_ids: Vec<OID>,
}

impl TestDb {
    fn new() -> Self {
        let path = unique_db_path();
        let connection = format!("sqlite://{}", path.display());
        mududb::sys::env_var::set_var("MUDU_CONNECTION", &connection);
        Self {
            path,
            session_ids: Vec::new(),
        }
    }

    fn open_session(&mut self) -> OID {
        let session_id = mudu_open().unwrap();
        self.session_ids.push(session_id);
        self.init_schema(session_id);
        session_id
    }

    fn init_schema(&self, xid: OID) {
        let ddl = include_str!("../../sql/ddl.sql");
        let init = include_str!("../../sql/init.sql");
        mudu_batch(xid, sql_stmt!(&ddl), sql_params!(&())).unwrap();
        mudu_batch(xid, sql_stmt!(&init), sql_params!(&())).unwrap();
    }

    fn query_wallet(&self, user_id: i32) -> Option<Wallets> {
        let rs: RecordSet<Wallets> = self.query_records(
            "SELECT user_id, balance, updated_at FROM wallets WHERE user_id = ?",
            &(user_id,),
        );
        rs.next_record().unwrap()
    }

    fn query_count<P: mududb::contract::database::sql_params::SQLParams>(
        &self,
        sql: &str,
        params: &P,
    ) -> i64 {
        let rs = mudu_query::<i64>(self.current_session(), sql_stmt!(&sql), params).unwrap();
        rs.next_record().unwrap().unwrap()
    }

    fn query_string<P: mududb::contract::database::sql_params::SQLParams>(
        &self,
        sql: &str,
        params: &P,
    ) -> Option<String> {
        let rs = mudu_query::<String>(self.current_session(), sql_stmt!(&sql), params).unwrap();
        rs.next_record().unwrap()
    }

    fn query_records<
        R: mududb::contract::database::entity::Entity,
        P: mududb::contract::database::sql_params::SQLParams,
    >(
        &self,
        sql: &str,
        params: &P,
    ) -> mududb::contract::database::entity_set::RecordSet<R> {
        mudu_query::<R>(self.current_session(), sql_stmt!(&sql), params).unwrap()
    }

    fn current_session(&self) -> OID {
        *self.session_ids.last().expect("test session not opened")
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        for session_id in self.session_ids.drain(..) {
            let _ = mudu_close(session_id);
        }
        mududb::sys::env_var::remove_var("MUDU_CONNECTION");
        let _ = mududb::sys::fs::sync::remove_file(&self.path);
    }
}

fn unique_db_path() -> PathBuf {
    let nanos = mududb::sys::time::system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    mududb::sys::env_var::temp_dir().join(format!("wallet-procedure-test-{nanos}.db"))
}
