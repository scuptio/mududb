//! End-to-end flow through the mock SQLite backend using SyscallPayload v1
//! (MSSP) frames on both the request and the response side.
#![cfg(feature = "mock-sqlite")]

use mudu_api_rust::mudu_sys;
use mudu_api_rust::types::UniCommandReturn;
use mudu_api_rust::{
    MockSqliteMuduSysCall, Mudu, UniCommandArgv, UniDataValue, UniOid, UniQueryArgv,
    UniScalarValue, UniSqlParam, UniSqlStmt,
};

fn temp_db_path(name: &str) -> std::path::PathBuf {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("mudu_api_rust_{name}_{suffix}.db"))
}

fn command_argv(sql: &str, params: Vec<UniDataValue>) -> UniCommandArgv {
    UniCommandArgv {
        oid: UniOid { h: 0, l: 0 },
        command: UniSqlStmt {
            sql_string: sql.to_string(),
        },
        param_list: UniSqlParam { params },
    }
}

#[tokio::test(flavor = "current_thread")]
async fn command_and_query_roundtrip_through_mssp_frames() {
    let db_path = temp_db_path("mssp_flow");
    MockSqliteMuduSysCall::set_database_path(&db_path);

    // Raw frame-level flow: MSSP request frame -> mock host -> MSSP response.
    let setup = command_argv(
        "create table demo (id integer primary key, name text not null)",
        Vec::new(),
    );
    let request = Mudu::serialize_command(&setup).unwrap();
    assert_eq!(&request[0..4], b"MSSP");
    let response = mudu_sys::command_raw(request).await.unwrap();
    assert_eq!(&response[0..4], b"MSSP");
    let ret = Mudu::deserialize_command(&response).unwrap();
    assert!(matches!(ret, UniCommandReturn::Ok(_)));

    let insert = command_argv(
        "insert into demo (id, name) values (?1, ?2)",
        vec![
            UniDataValue::Scalar(UniScalarValue::I64(1)),
            UniDataValue::Scalar(UniScalarValue::String("alice".to_string())),
        ],
    );
    let response = Mudu::command(&insert).await.unwrap();
    assert_eq!(response.affected_rows(), Some(1));

    // The query response decodes from an MSSP frame with the
    // `[ok_tag, UniQueryResult]` variant body.
    let query = UniQueryArgv {
        oid: UniOid { h: 0, l: 0 },
        query: UniSqlStmt {
            sql_string: "select id, name from demo where id = ?1".to_string(),
        },
        param_list: UniSqlParam {
            params: vec![UniDataValue::Scalar(UniScalarValue::I64(1))],
        },
    };
    let request = Mudu::serialize_query(&query).unwrap();
    assert_eq!(&request[0..4], b"MSSP");
    let response = mudu_sys::query_raw(request).await.unwrap();
    let ret = Mudu::deserialize_query(&response).unwrap();
    let result = match ret {
        mudu_api_rust::UniQueryReturn::Ok(result) => result,
        mudu_api_rust::UniQueryReturn::Err(error) => panic!("query failed: {}", error.err_msg),
    };
    assert_eq!(result.result_set.row_set.len(), 1);
    let row = &result.result_set.row_set[0];
    assert!(matches!(
        &row.fields[0],
        UniDataValue::Scalar(UniScalarValue::I64(1))
    ));
    assert!(matches!(
        &row.fields[1],
        UniDataValue::Scalar(UniScalarValue::String(name)) if name == "alice"
    ));
    assert_eq!(result.tuple_desc.record_fields.len(), 2);

    // Error frames also decode: an unknown table surfaces as the `Err`
    // variant with the host message.
    let bad = command_argv("insert into missing_table values (1)", Vec::new());
    let response = Mudu::command(&bad).await.unwrap();
    let error = response.require_ok().unwrap_err();
    assert!(error.err_msg.contains("missing_table"));

    let _ = std::fs::remove_file(&db_path);
}
