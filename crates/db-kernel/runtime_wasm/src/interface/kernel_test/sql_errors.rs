use super::*;

#[test]
fn query_internal_reports_decode_error_for_invalid_bytes() {
    let bytes = b"not a valid query payload";
    let out = query_internal(bytes);
    assert!(!out.is_empty());
    let result = mudu_binding::system::query_invoke::deserialize_query_result(&out);
    match result {
        Err(err) => assert_eq!(err.ec(), mudu::error::ErrorCode::CorruptedData),
        Ok(_) => panic!("expected error"),
    }
}

#[test]
fn query_internal_reports_missing_context_error() {
    let bytes =
        mudu_binding::system::query_invoke::serialize_query_dyn_param(999u128, &"SELECT 1", &())
            .unwrap();
    let out = query_internal(&bytes);
    assert!(!out.is_empty());
    let result = mudu_binding::system::query_invoke::deserialize_query_result(&out);
    match result {
        Err(err) => assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound),
        Ok(_) => panic!("expected error"),
    }
}

#[test]
fn command_internal_reports_missing_context_error() {
    let bytes = mudu_binding::system::command_invoke::serialize_command_param(
        999u128,
        &"INSERT INTO t VALUES (1)",
        &(),
    )
    .unwrap();
    let out = command_internal(&bytes);
    assert!(!out.is_empty());
    let err = mudu_binding::system::command_invoke::deserialize_command_result(&out).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
}

#[test]
fn batch_internal_reports_missing_context_error() {
    let bytes = mudu_binding::system::command_invoke::serialize_command_param(
        999u128,
        &"INSERT INTO t VALUES (1)",
        &(),
    )
    .unwrap();
    let out = batch_internal(&bytes);
    assert!(!out.is_empty());
    let err = mudu_binding::system::command_invoke::deserialize_command_result(&out).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
}

#[test]
fn empty_sql_syscalls_return_empty() {
    assert!(empty_query_internal(b"ignored").is_empty());
    assert!(empty_command_internal(b"ignored").is_empty());
}

#[tokio::test]
async fn async_query_reports_missing_context_error() {
    let bytes =
        mudu_binding::system::query_invoke::serialize_query_dyn_param(999u128, &"SELECT 1", &())
            .unwrap();
    let out = async_query_internal(bytes).await;
    let result = mudu_binding::system::query_invoke::deserialize_query_result(&out);
    match result {
        Err(err) => assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound),
        Ok(_) => panic!("expected error"),
    }
}

#[tokio::test]
async fn async_command_reports_missing_context_error() {
    let bytes = mudu_binding::system::command_invoke::serialize_command_param(
        999u128,
        &"INSERT INTO t VALUES (1)",
        &(),
    )
    .unwrap();
    let out = async_command_internal(bytes).await;
    let err = mudu_binding::system::command_invoke::deserialize_command_result(&out).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
}

#[tokio::test]
async fn async_batch_reports_missing_context_error() {
    let bytes = mudu_binding::system::command_invoke::serialize_command_param(
        999u128,
        &"INSERT INTO t VALUES (1)",
        &(),
    )
    .unwrap();
    let out = async_batch_internal(bytes).await;
    let err = mudu_binding::system::command_invoke::deserialize_command_result(&out).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
}
