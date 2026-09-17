#![allow(clippy::unwrap_used)]

use super::AppInstImpl;
use crate::backend::mudud_cfg::ServerMode;
use crate::service::app_inst::AppInst;
use crate::service::app_package::AppPackage;
use crate::service::runtime_opt::{ComponentTarget, RuntimeOpt};
use crate::service::test_wasm_mod_path::wasm_mod_path;
use crate::service::wt_runtime_component::WTRuntimeComponent;
use mudu::common::app_info::AppInfo;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::tuple::tuple_datum::TupleDatum;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

fn test_proc_desc(module_name: &str, proc_name: &str) -> ProcDesc {
    ProcDesc::new(
        module_name.to_string(),
        proc_name.to_string(),
        <()>::tuple_desc_static(&[]),
        <()>::tuple_desc_static(&[]),
        false,
    )
}

fn test_package(desc: ModProcDesc) -> AppPackage {
    AppPackage {
        package_cfg: AppInfo {
            name: "app".to_string(),
            lang: "rust".to_string(),
            version: "0.1.0".to_string(),
            use_async: false,
        },
        ddl_sql: "CREATE TABLE t(id INTEGER PRIMARY KEY);".to_string(),
        package_desc: desc,
        initdb_sql: String::new(),
        modules: HashMap::new(),
    }
}

fn temp_db_path(label: &str) -> String {
    let nanos = mudu_sys::time::system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = mudu_sys::env_var::temp_dir().join(format!("mudu-app-inst-{label}-{nanos}"));
    mudu_sys::fs::sync::create_dir_all(&path).unwrap();
    path.to_str().unwrap().to_string()
}

// libsql performs real SQLite file IO outside `mudu_sys`; the
// deterministic-simulation backend keeps fs writes in memory,
// so the database file cannot be created. Native backend only.
#[cfg(not(feature = "ds"))]
#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn accessors_and_connection_lifecycle() {
    let db_path = temp_db_path("conn");
    let package = test_package(ModProcDesc::new(HashMap::new()));
    let app = AppInstImpl::build(
        &db_path,
        &package,
        vec![],
        ComponentTarget::P2,
        false,
        ServerMode::Legacy,
        None,
        false,
    )
    .await
    .unwrap();

    assert_eq!(app.name(), "app");
    assert!(!app.schema_mgr().table_names().is_empty());
    assert!(app.async_runtime().is_none());
    assert!(app.connection(1).is_none());

    app.create_conn(1).await.unwrap();
    assert!(app.connection(1).is_some());

    app.remove_conn(1).unwrap();
    assert!(app.connection(1).is_none());
}

// libsql performs real SQLite file IO outside `mudu_sys`; the
// deterministic-simulation backend keeps fs writes in memory,
// so the database file cannot be created. Native backend only.
#[cfg(not(feature = "ds"))]
#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn procedure_and_describe_return_not_found_for_missing_module() {
    let db_path = temp_db_path("missing");
    let package = test_package(ModProcDesc::new(HashMap::new()));
    let app = AppInstImpl::build(
        &db_path,
        &package,
        vec![],
        ComponentTarget::P2,
        false,
        ServerMode::Legacy,
        None,
        false,
    )
    .await
    .unwrap();

    assert!(app.procedure("missing", "missing").is_none());

    let err = app.describe("missing", "missing").unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
}

// libsql performs real SQLite file IO outside `mudu_sys`; the
// deterministic-simulation backend keeps fs writes in memory,
// so the database file cannot be created. Native backend only.
#[cfg(not(feature = "ds"))]
#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn invoke_rejects_missing_procedure() {
    let db_path = temp_db_path("invoke");
    let package = test_package(ModProcDesc::new(HashMap::new()));
    let app = AppInstImpl::build(
        &db_path,
        &package,
        vec![],
        ComponentTarget::P2,
        false,
        ServerMode::Legacy,
        None,
        false,
    )
    .await
    .unwrap();

    let param = ProcedureParam::new(0, 0, vec![]);
    let err = app
        .invoke(1, "missing", "missing", param, None)
        .await
        .unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
}

// libsql performs real SQLite file IO outside `mudu_sys`; the
// deterministic-simulation backend keeps fs writes in memory,
// so the database file cannot be created. Native backend only.
#[cfg(not(feature = "ds"))]
#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn invoke_async_rejected_when_async_disabled() {
    let db_path = temp_db_path("async-disabled");
    let mut desc_map = HashMap::new();
    desc_map.insert("mod_0".to_string(), vec![test_proc_desc("mod_0", "proc")]);
    let package = test_package(ModProcDesc::new(desc_map));
    let app = AppInstImpl::build(
        &db_path,
        &package,
        vec![],
        ComponentTarget::P2,
        false,
        ServerMode::Legacy,
        None,
        false,
    )
    .await
    .unwrap();

    let param = ProcedureParam::new(0, 0, vec![]);
    let err = app
        .invoke_async(1, "mod_0", "proc", param, None)
        .await
        .unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::Database);
    assert!(err.to_string().contains("enable async mode"));
}

/// Build a real wallet-package instance (compiled WASM modules, libsql-backed
/// storage) in async mode. Returns the app and its db path so tests can
/// verify committed state through a separate connection.
///
/// The wallet package is used because its procedures perform real SQL host
/// calls (`query`/`command`): app1's procs make no host calls and its
/// async-lifted `proc_sys_call_mtp` fails before its first host call even
/// through the full invoke path (a pre-existing fixture issue).
#[cfg(not(feature = "ds"))]
async fn build_wallet_async_app(label: &str) -> (AppInstImpl, String) {
    let package = AppPackage::load(
        PathBuf::from(wasm_mod_path())
            .join("../../../testing/mpk/wallet.mpk")
            .canonicalize()
            .unwrap(),
    )
    .unwrap();
    let mut runtime = WTRuntimeComponent::build(&RuntimeOpt {
        component_target: ComponentTarget::P2,
        enable_async: true,
        sever_mode: Default::default(),
        async_runtime: None,
        defer_initdb: false,
    })
    .unwrap();
    runtime.instantiate().unwrap();
    let modules = runtime.compile_modules(&package).unwrap();
    let db_path = temp_db_path(label);
    let app = AppInstImpl::build(
        &db_path,
        &package,
        modules,
        ComponentTarget::P2,
        true,
        ServerMode::Legacy,
        None,
        false,
    )
    .await
    .unwrap();
    (app, db_path)
}

#[cfg(not(feature = "ds"))]
fn deposit_param(user_id: i32, amount: i32) -> ProcedureParam {
    ProcedureParam::from_tuple(0, (user_id, amount), &<(i32, i32)>::tuple_desc_static(&[])).unwrap()
}

#[cfg(not(feature = "ds"))]
fn create_user_param(user_id: i32, name: &str) -> ProcedureParam {
    ProcedureParam::from_tuple(
        0,
        (user_id, name.to_string(), format!("{name}@example.com")),
        &<(i32, String, String)>::tuple_desc_static(&[]),
    )
    .unwrap()
}

/// The committed balance of `user_id`, read through a fresh connection to the
/// app's database — cross-connection visibility proves the commit is durable.
#[cfg(not(feature = "ds"))]
async fn wallet_balance(db_path: &str, user_id: i32) -> i32 {
    use mudu_contract::database::result_batch::ResultBatch;
    use mudu_contract::database::sql::DBConn;

    let conn = crate::db_connector::DBConnector::connect(&format!(
        "db={} app=wallet db_type=LibSQLAsync",
        db_path
    ))
    .await
    .unwrap();
    let async_conn = match conn {
        DBConn::Async(async_conn) => async_conn,
        DBConn::Sync(_) => panic!("LibSQLAsync connection must be async"),
    };
    let rs = async_conn
        .query(
            Box::new(format!(
                "SELECT balance FROM wallets WHERE user_id = {user_id}"
            )),
            Box::new(()),
        )
        .await
        .unwrap();
    let batch = ResultBatch::from_result_set_async(0, rs.as_ref())
        .await
        .unwrap();
    let rows = batch.rows();
    assert_eq!(rows.len(), 1, "user {user_id} must have a wallet row");
    rows[0].values()[0].to_i32()
}

// libsql performs real SQLite file IO outside `mudu_sys`. Native backend only.
#[cfg(not(feature = "ds"))]
#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn concurrent_invoke_async_transactions_commit_independently() {
    let (app, db_path) = build_wallet_async_app("tx-commit").await;
    let task_a = app.task_create().await.unwrap();
    let task_b = app.task_create().await.unwrap();

    // Two invocations interleaved on one task set (`join!` polls both on this
    // thread): each must get its own transaction context at pre_invoke, and
    // one's commit must not disturb the other.
    let (result_a, result_b) = tokio::join!(
        app.invoke_async(task_a, "wallet", "deposit", deposit_param(1, 100), None),
        app.invoke_async(task_b, "wallet", "deposit", deposit_param(2, 200), None),
    );
    result_a.unwrap();
    result_b.unwrap();

    // Both transactions committed durably and independently. (The wallet
    // seed data gives every user an initial balance of 10000.)
    assert_eq!(wallet_balance(&db_path, 1).await, 10100);
    assert_eq!(wallet_balance(&db_path, 2).await, 10200);

    app.task_end(task_a).unwrap();
    app.task_end(task_b).unwrap();
}

// libsql performs real SQLite file IO outside `mudu_sys`. Native backend only.
#[cfg(not(feature = "ds"))]
#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn failing_invoke_async_rolls_back_only_its_own_tx() {
    let (app, db_path) = build_wallet_async_app("tx-rollback").await;
    let task_bad = app.task_create().await.unwrap();
    let task_good = app.task_create().await.unwrap();

    // User 1 exists in the seed data, so re-creating it hits a UNIQUE
    // constraint inside the invocation's transaction — a mid-transaction
    // failure after real host I/O.
    let (bad, good) = tokio::join!(
        app.invoke_async(
            task_bad,
            "wallet",
            "create_user",
            create_user_param(1, "alice-clone"),
            None,
        ),
        app.invoke_async(task_good, "wallet", "deposit", deposit_param(2, 200), None),
    );
    let err = bad.unwrap_err();
    assert!(
        err.to_string().contains("UNIQUE constraint"),
        "unexpected error: {err}"
    );
    good.unwrap();

    // The concurrent invocation's transaction committed; the failed one left
    // no partial state and did not poison its own task connection.
    assert_eq!(wallet_balance(&db_path, 2).await, 10200);
    app.invoke_async(task_bad, "wallet", "deposit", deposit_param(1, 50), None)
        .await
        .unwrap();
    assert_eq!(wallet_balance(&db_path, 1).await, 10050);

    app.task_end(task_bad).unwrap();
    app.task_end(task_good).unwrap();
}
