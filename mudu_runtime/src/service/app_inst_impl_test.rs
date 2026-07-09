#![allow(clippy::unwrap_used)]

use super::AppInstImpl;
use crate::backend::mudud_cfg::ServerMode;
use crate::service::app_inst::AppInst;
use crate::service::app_package::AppPackage;
use crate::service::runtime_opt::ComponentTarget;
use mudu::common::app_info::AppInfo;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::tuple::tuple_datum::TupleDatum;
use std::collections::HashMap;
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
    )
    .await
    .unwrap();

    assert!(app.procedure("missing", "missing").is_none());

    let err = app.describe("missing", "missing").unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
}

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
