use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_binding::codec::handle_sys_session;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::result_set::ResultSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_sys::sync::SMutex;
use std::sync::Arc;

#[allow(unused)]
pub fn invoke_host_command<F>(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams, f: F) -> RS<u64>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary =
        mudu_binding::system::command_invoke::serialize_command_param(oid, sql, params)?;
    let result = f(param_binary)?;
    let affected_rows = mudu_binding::system::command_invoke::deserialize_command_result(&result)?;
    Ok(affected_rows)
}

#[allow(unused)]
pub fn invoke_host_batch<F>(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams, f: F) -> RS<u64>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    invoke_host_command(oid, sql, params, f)
}

#[allow(unused)]
pub fn invoke_host_query<R: Entity, F>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
    f: F,
) -> RS<RecordSet<R>>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary =
        mudu_binding::system::query_invoke::serialize_query_dyn_param(oid, sql, params)?;
    let result = f(param_binary)?;
    let (result_batch, tuple_desc) =
        mudu_binding::system::query_invoke::deserialize_query_result(&result)?;
    let record_set = RecordSet::<R>::new(
        Arc::new(ResultSetWrapper::new(result_batch)),
        Arc::new(tuple_desc),
    );
    Ok(record_set)
}

pub fn serialize_get_param(key: &[u8]) -> Vec<u8> {
    handle_sys_session::serialize_get_param(key)
}

pub fn serialize_session_get_param(session_id: OID, key: &[u8]) -> Vec<u8> {
    handle_sys_session::serialize_session_get_param(session_id, key)
}

pub fn deserialize_get_param(input: &[u8]) -> RS<Vec<u8>> {
    handle_sys_session::deserialize_get_param(input)
}

pub fn deserialize_session_get_param(input: &[u8]) -> RS<(OID, Vec<u8>)> {
    handle_sys_session::deserialize_session_get_param(input)
}

pub fn serialize_get_result(value: Option<&[u8]>) -> Vec<u8> {
    handle_sys_session::serialize_get_result(value)
}

pub fn deserialize_get_result(input: &[u8]) -> RS<Option<Vec<u8>>> {
    handle_sys_session::deserialize_get_result(input)
}

pub fn serialize_put_param(key: &[u8], value: &[u8]) -> Vec<u8> {
    handle_sys_session::serialize_put_param(key, value)
}

pub fn serialize_session_put_param(session_id: OID, key: &[u8], value: &[u8]) -> Vec<u8> {
    handle_sys_session::serialize_session_put_param(session_id, key, value)
}

pub fn deserialize_put_param(input: &[u8]) -> RS<(Vec<u8>, Vec<u8>)> {
    handle_sys_session::deserialize_put_param(input)
}

pub fn deserialize_session_put_param(input: &[u8]) -> RS<(OID, Vec<u8>, Vec<u8>)> {
    handle_sys_session::deserialize_session_put_param(input)
}

pub fn serialize_put_result() -> Vec<u8> {
    handle_sys_session::serialize_put_result()
}

pub fn deserialize_put_result(input: &[u8]) -> RS<()> {
    handle_sys_session::deserialize_put_result(input)
}

pub fn serialize_range_param(start_key: &[u8], end_key: &[u8]) -> Vec<u8> {
    handle_sys_session::serialize_range_param(start_key, end_key)
}

pub fn serialize_session_range_param(session_id: OID, start_key: &[u8], end_key: &[u8]) -> Vec<u8> {
    handle_sys_session::serialize_session_range_param(session_id, start_key, end_key)
}

pub fn deserialize_range_param(input: &[u8]) -> RS<(Vec<u8>, Vec<u8>)> {
    handle_sys_session::deserialize_range_param(input)
}

pub fn deserialize_session_range_param(input: &[u8]) -> RS<(OID, Vec<u8>, Vec<u8>)> {
    handle_sys_session::deserialize_session_range_param(input)
}

pub fn serialize_open_param() -> Vec<u8> {
    handle_sys_session::serialize_open_param()
}

pub fn serialize_open_argv_param(argv: &UniSessionOpenArgv) -> Vec<u8> {
    handle_sys_session::serialize_open_argv_param(argv)
}

pub fn deserialize_open_param(input: &[u8]) -> RS<UniSessionOpenArgv> {
    handle_sys_session::deserialize_open_param(input)
}

pub fn serialize_open_result(session_id: OID) -> Vec<u8> {
    handle_sys_session::serialize_open_result(session_id)
}

pub fn deserialize_open_result(input: &[u8]) -> RS<OID> {
    handle_sys_session::deserialize_open_result(input)
}

pub fn serialize_close_param(session_id: OID) -> Vec<u8> {
    handle_sys_session::serialize_close_param(session_id)
}

pub fn deserialize_close_param(input: &[u8]) -> RS<OID> {
    handle_sys_session::deserialize_close_param(input)
}

pub fn serialize_close_result() -> Vec<u8> {
    handle_sys_session::serialize_close_result()
}

pub fn deserialize_close_result(input: &[u8]) -> RS<()> {
    handle_sys_session::deserialize_close_result(input)
}

pub fn serialize_range_result(items: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    handle_sys_session::serialize_range_result(items)
}

pub fn deserialize_range_result(input: &[u8]) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    handle_sys_session::deserialize_range_result(input)
}

pub fn invoke_host_get<F>(key: &[u8], f: F) -> RS<Option<Vec<u8>>>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_get_param(key);
    let result = f(param_binary)?;
    deserialize_get_result(&result)
}

pub fn invoke_host_open<F>(f: F) -> RS<OID>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_open_param();
    let result = f(param_binary)?;
    deserialize_open_result(&result)
}

pub fn invoke_host_open_argv<F>(argv: &UniSessionOpenArgv, f: F) -> RS<OID>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_open_argv_param(argv);
    let result = f(param_binary)?;
    deserialize_open_result(&result)
}

pub fn invoke_host_close<F>(session_id: OID, f: F) -> RS<()>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_close_param(session_id);
    let result = f(param_binary)?;
    deserialize_close_result(&result)
}

pub fn invoke_host_session_get<F>(session_id: OID, key: &[u8], f: F) -> RS<Option<Vec<u8>>>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_session_get_param(session_id, key);
    let result = f(param_binary)?;
    deserialize_get_result(&result)
}

pub fn invoke_host_session_put<F>(session_id: OID, key: &[u8], value: &[u8], f: F) -> RS<()>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_session_put_param(session_id, key, value);
    let result = f(param_binary)?;
    deserialize_put_result(&result)
}

pub fn invoke_host_session_range<F>(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
    f: F,
) -> RS<Vec<(Vec<u8>, Vec<u8>)>>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_session_range_param(session_id, start_key, end_key);
    let result = f(param_binary)?;
    deserialize_range_result(&result)
}

pub fn invoke_host_put<F>(key: &[u8], value: &[u8], f: F) -> RS<()>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_put_param(key, value);
    let result = f(param_binary)?;
    deserialize_put_result(&result)
}

pub fn invoke_host_range<F>(start_key: &[u8], end_key: &[u8], f: F) -> RS<Vec<(Vec<u8>, Vec<u8>)>>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_range_param(start_key, end_key);
    let result = f(param_binary)?;
    deserialize_range_result(&result)
}

#[allow(unused)]
pub async fn async_invoke_host_command<F>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
    f: F,
) -> RS<u64>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary =
        mudu_binding::system::command_invoke::serialize_command_param(oid, sql, params)?;
    let result = f(param_binary).await?;
    let affected_rows = mudu_binding::system::command_invoke::deserialize_command_result(&result)?;
    Ok(affected_rows)
}

#[allow(unused)]
pub async fn async_invoke_host_batch<F>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
    f: F,
) -> RS<u64>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    async_invoke_host_command(oid, sql, params, f).await
}

#[allow(unused)]
pub async fn async_invoke_host_query<R: Entity, F>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
    f: F,
) -> RS<RecordSet<R>>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary =
        mudu_binding::system::query_invoke::serialize_query_dyn_param(oid, sql, params)?;
    let result = f(param_binary).await?;
    let (result_batch, tuple_desc) =
        mudu_binding::system::query_invoke::deserialize_query_result(&result)?;
    let record_set = RecordSet::<R>::new(
        Arc::new(ResultSetWrapper::new(result_batch)),
        Arc::new(tuple_desc),
    );
    Ok(record_set)
}

pub async fn async_invoke_host_get<F>(key: &[u8], f: F) -> RS<Option<Vec<u8>>>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_get_param(key);
    let result = f(param_binary).await?;
    deserialize_get_result(&result)
}

pub async fn async_invoke_host_open<F>(f: F) -> RS<OID>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_open_param();
    let result = f(param_binary).await?;
    deserialize_open_result(&result)
}

pub async fn async_invoke_host_open_argv<F>(argv: &UniSessionOpenArgv, f: F) -> RS<OID>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_open_argv_param(argv);
    let result = f(param_binary).await?;
    deserialize_open_result(&result)
}

pub async fn async_invoke_host_close<F>(session_id: OID, f: F) -> RS<()>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_close_param(session_id);
    let result = f(param_binary).await?;
    deserialize_close_result(&result)
}

pub async fn async_invoke_host_session_get<F>(
    session_id: OID,
    key: &[u8],
    f: F,
) -> RS<Option<Vec<u8>>>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_session_get_param(session_id, key);
    let result = f(param_binary).await?;
    deserialize_get_result(&result)
}

pub async fn async_invoke_host_session_put<F>(
    session_id: OID,
    key: &[u8],
    value: &[u8],
    f: F,
) -> RS<()>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_session_put_param(session_id, key, value);
    let result = f(param_binary).await?;
    deserialize_put_result(&result)
}

pub async fn async_invoke_host_session_range<F>(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
    f: F,
) -> RS<Vec<(Vec<u8>, Vec<u8>)>>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_session_range_param(session_id, start_key, end_key);
    let result = f(param_binary).await?;
    deserialize_range_result(&result)
}

pub async fn async_invoke_host_put<F>(key: &[u8], value: &[u8], f: F) -> RS<()>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_put_param(key, value);
    let result = f(param_binary).await?;
    deserialize_put_result(&result)
}

pub async fn async_invoke_host_range<F>(
    start_key: &[u8],
    end_key: &[u8],
    f: F,
) -> RS<Vec<(Vec<u8>, Vec<u8>)>>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = serialize_range_param(start_key, end_key);
    let result = f(param_binary).await?;
    deserialize_range_result(&result)
}

pub struct ResultSetWrapper {
    batch: SMutex<ResultBatch>,
}

impl ResultSetWrapper {
    pub fn new(batch: ResultBatch) -> ResultSetWrapper {
        ResultSetWrapper {
            batch: SMutex::new(batch),
        }
    }
}

impl ResultSet for ResultSetWrapper {
    fn next(&self) -> RS<Option<TupleValue>> {
        let mut batch = self.batch.lock().unwrap();
        let t = batch.mut_rows().pop();
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mudu_binding::system::{command_invoke, query_invoke};
    use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
    use mudu_contract::database::sql_stmt_text::SQLStmtText;
    use mudu_contract::tuple::tuple_datum::TupleDatum;
    use mudu_contract::tuple::tuple_value::TupleValue;
    use mudu_type::dat_value::DatValue;

    #[test]
    fn kv_get_roundtrip() {
        let encoded = serialize_get_param(b"k1");
        let decoded = deserialize_get_param(&encoded).unwrap();
        assert_eq!(decoded, b"k1");

        let encoded_result = serialize_get_result(Some(b"v1"));
        let decoded_result = deserialize_get_result(&encoded_result).unwrap();
        assert_eq!(decoded_result, Some(b"v1".to_vec()));
    }

    #[test]
    fn kv_range_roundtrip() {
        let encoded = serialize_range_param(b"a", b"z");
        let decoded = deserialize_range_param(&encoded).unwrap();
        assert_eq!(decoded, (b"a".to_vec(), b"z".to_vec()));

        let encoded_result = serialize_range_result(&[
            (b"a".to_vec(), b"1".to_vec()),
            (b"b".to_vec(), b"2".to_vec()),
        ]);
        let decoded_result = deserialize_range_result(&encoded_result).unwrap();
        assert_eq!(
            decoded_result,
            vec![
                (b"a".to_vec(), b"1".to_vec()),
                (b"b".to_vec(), b"2".to_vec())
            ]
        );
    }

    #[test]
    fn open_and_open_argv_helpers_roundtrip() {
        let oid = invoke_host_open(|_| Ok(serialize_open_result(15))).unwrap();
        assert_eq!(oid, 15);

        let argv = UniSessionOpenArgv::new(7);
        let oid = invoke_host_open_argv(&argv, |input| {
            let decoded = deserialize_open_param(&input).unwrap();
            assert_eq!(decoded.worker_oid(), 7);
            Ok(serialize_open_result(21))
        })
        .unwrap();
        assert_eq!(oid, 21);
    }

    #[test]
    fn command_and_query_helpers_decode_serialized_results() {
        let stmt = SQLStmtText::new("SELECT 1".to_string());

        let affected = invoke_host_command(3, &stmt, &(), |input| {
            let (oid, _, _) = command_invoke::deserialize_command_param(&input).unwrap();
            assert_eq!(oid, 3);
            Ok(command_invoke::serialize_command_result(Ok(5)))
        })
        .unwrap();
        assert_eq!(affected, 5);

        let records = invoke_host_query::<i32, _>(4, &stmt, &(), |input| {
            let (oid, _, _) = query_invoke::deserialize_query_param(&input).unwrap();
            assert_eq!(oid, 4);
            Ok(query_invoke::serialize_query_result(Ok((
                mudu_contract::database::result_batch::ResultBatch::from(
                    4,
                    vec![TupleValue::from(vec![DatValue::from_i32(8)])],
                    true,
                ),
                <i32 as TupleDatum>::tuple_desc_static(&["value".to_string()]),
            ))))
        })
        .unwrap();
        assert_eq!(records.next_record().unwrap(), Some(8));
    }

    #[test]
    fn async_host_helpers_roundtrip_sync_payload_shapes() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let stmt = SQLStmtText::new("SELECT 1".to_string());

        let oid = async_invoke_host_open(|_| async { Ok(serialize_open_result(31)) })
            .await
            .unwrap();
        assert_eq!(oid, 31);

        let affected = async_invoke_host_batch(6, &stmt, &(), |input: Vec<u8>| async move {
            let (oid, _, _) = command_invoke::deserialize_command_param(&input).unwrap();
            assert_eq!(oid, 6);
            Ok(command_invoke::serialize_command_result(Ok(2)))
        })
        .await
        .unwrap();
        assert_eq!(affected, 2);

        let records =
            async_invoke_host_query::<i32, _>(8, &stmt, &(), |input: Vec<u8>| async move {
                let (oid, _, _) = query_invoke::deserialize_query_param(&input).unwrap();
                assert_eq!(oid, 8);
                Ok(query_invoke::serialize_query_result(Ok((
                    mudu_contract::database::result_batch::ResultBatch::from(
                        8,
                        vec![TupleValue::from(vec![DatValue::from_i32(13)])],
                        true,
                    ),
                    <i32 as TupleDatum>::tuple_desc_static(&["value".to_string()]),
                ))))
            })
            .await
            .unwrap();
        assert_eq!(records.next_record().unwrap(), Some(13));

        let got = async_invoke_host_session_get(9, b"k", |input: Vec<u8>| async move {
            let (oid, key) = deserialize_session_get_param(&input).unwrap();
            assert_eq!(oid, 9);
            assert_eq!(key, b"k");
            Ok(serialize_get_result(Some(b"v")))
        })
        .await
        .unwrap();
        assert_eq!(got, Some(b"v".to_vec()));
        }).unwrap();
    }
}
