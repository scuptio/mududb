use crate::rust::procedures;
use crate::rust::transactions::object::Transactions;
use crate::rust::wallets::object::Wallets;
use mudu::common::id::OID;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::{sql_params, sql_stmt};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use sys_interface::sync_api::{mudu_batch, mudu_close, mudu_open, mudu_query};

static TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

#[test]
fn create_update_and_delete_user() {
    let _guard = test_mutex().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut db = TestDb::new();
    let xid = db.open_session();

    procedures::create_user(xid, 3, "Carol".to_string(), "carol@example.com".to_string()).unwrap();

    assert_eq!(db.query_count("SELECT COUNT(*) FROM users WHERE user_id = ?", &(3,)), 1);
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
    assert_eq!(db.query_count("SELECT COUNT(*) FROM users WHERE user_id = ?", &(3,)), 0);
    assert!(db.query_wallet(3).is_none());
}

#[test]
fn delete_user_rejects_non_zero_balance() {
    let _guard = test_mutex().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut db = TestDb::new();
    let xid = db.open_session();

    let err = procedures::delete_user(xid, 1).unwrap_err();
    assert!(
        err.message().contains("non-zero balance"),
        "unexpected error: {err:?}"
    );
}

#[test]
fn transfer_funds_moves_balance_and_writes_transaction() {
    let _guard = test_mutex().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
fn transfer_rejects_self_transfer() {
    let _guard = test_mutex().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut db = TestDb::new();
    let xid = db.open_session();

    let err = procedures::transfer(xid, 1, 1, 100).unwrap_err();
    assert!(err.message().contains("self"), "unexpected error: {err:?}");
}

#[test]
fn deposit_withdraw_and_purchase_update_balance_and_transactions() {
    let _guard = test_mutex().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
fn withdraw_rejects_insufficient_funds() {
    let _guard = test_mutex().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut db = TestDb::new();
    let xid = db.open_session();

    let err = procedures::withdraw(xid, 1, 20000).unwrap_err();
    assert!(
        err.message().contains("Insufficient funds"),
        "unexpected error: {err:?}"
    );
}

fn test_mutex() -> &'static Mutex<()> {
    TEST_MUTEX.get_or_init(|| Mutex::new(()))
}

struct TestDb {
    path: PathBuf,
    session_ids: Vec<OID>,
}

impl TestDb {
    fn new() -> Self {
        let path = unique_db_path();
        let connection = format!("sqlite://{}", path.display());
        unsafe {
            std::env::set_var("MUDU_CONNECTION", connection);
        }
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

    #[allow(dead_code)]
    fn query_transaction_by_type(&self, trans_type: &str) -> Option<Transactions> {
        let rs: RecordSet<Transactions> = self.query_records(
            "SELECT trans_id, trans_type, from_user, to_user, amount, created_at FROM transactions WHERE trans_type = ? LIMIT 1",
            &(trans_type.to_string(),),
        );
        rs.next_record().unwrap()
    }

    fn query_count<P: mudu_contract::database::sql_params::SQLParams>(&self, sql: &str, params: &P) -> i64 {
        let rs = mudu_query::<i64>(self.current_session(), sql_stmt!(&sql), params).unwrap();
        rs.next_record().unwrap().unwrap()
    }

    fn query_string<P: mudu_contract::database::sql_params::SQLParams>(
        &self,
        sql: &str,
        params: &P,
    ) -> Option<String> {
        let rs = mudu_query::<String>(self.current_session(), sql_stmt!(&sql), params).unwrap();
        rs.next_record().unwrap()
    }

    fn query_records<R: mudu_contract::database::entity::Entity, P: mudu_contract::database::sql_params::SQLParams>(
        &self,
        sql: &str,
        params: &P,
    ) -> mudu_contract::database::entity_set::RecordSet<R> {
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
        unsafe {
            std::env::remove_var("MUDU_CONNECTION");
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

fn unique_db_path() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("wallet-procedure-test-{nanos}.db"))
}

