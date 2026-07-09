//! JSON-facing wrapper around the async MuduDB client.
//!
//! Accepts and returns `serde_json::Value` bodies, converting to and from the
//! universal value encoding used by MuduDB's KV and procedure APIs.

use crate::client::async_client::{AsyncClient, AsyncClientImpl};
use base64::Engine;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_data_value::UniDataValue;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_binding::universal::uni_scalar_value::UniScalarValue;
use mudu_contract::protocol::{
    ClientRequest, GetRequest, KeyValue, ProcedureInvokeRequest, PutRequest, RangeScanRequest,
    ServerResponse,
};
use mudu_type::datum::DatumDyn;
use mudu_type::type_family::TypeFamily;
use serde::Deserialize;
use serde::de::{self, Deserializer};
use serde_json::{Value, json};

/// JSON wrapper around an async client.
///
/// `C` is the underlying client type (usually `AsyncClientImpl`).
pub struct JsonClient<C> {
    inner: C,
}

impl<C> JsonClient<C> {
    /// Wrap `inner` in a new `JsonClient`.
    pub fn new(inner: C) -> Self {
        Self { inner }
    }

    /// Consume `self` and return the underlying client.
    pub fn into_inner(self) -> C {
        self.inner
    }
}

impl JsonClient<AsyncClientImpl> {
    /// Connect to `addr` and wrap the resulting client.
    pub async fn connect(addr: &str) -> RS<Self> {
        Ok(Self::new(AsyncClientImpl::connect(addr).await?))
    }
}

impl<C> JsonClient<C>
where
    C: AsyncClient,
{
    /// Send a SQL query or execute request encoded as JSON.
    pub async fn command(&mut self, request: Value) -> RS<Value> {
        let request = serde_json::from_value::<JsonCommandRequest>(request)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode json command request error", e))?;
        let client_request = ClientRequest::new(request.app_name, request.sql);
        let response = if request.kind == Some(CommandKind::Execute) {
            self.inner.execute(client_request).await?
        } else {
            self.inner.query(client_request).await?
        };
        server_response_to_json(&response)
    }

    /// Put a key-value item from a JSON request body.
    pub async fn put(&mut self, request: Value) -> RS<Value> {
        let request = serde_json::from_value::<JsonPutRequest>(request)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode json put request error", e))?;
        let response = self
            .inner
            .put(PutRequest::new(
                request.oid.to_oid(),
                json_value_to_universal_bytes(request.key)?,
                json_value_to_universal_bytes(request.value)?,
            ))
            .await?;
        Ok(json!({ "ok": response.ok() }))
    }

    /// Get a value by key from a JSON request body.
    pub async fn get(&mut self, request: Value) -> RS<Value> {
        let request = serde_json::from_value::<JsonGetRequest>(request)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode json get request error", e))?;
        let response = self
            .inner
            .get(GetRequest::new(
                request.oid.to_oid(),
                json_value_to_universal_bytes(request.key)?,
            ))
            .await?;
        match response.into_value() {
            Some(value) => universal_bytes_to_json_value(&value),
            None => Ok(Value::Null),
        }
    }

    /// Scan a key range from a JSON request body.
    pub async fn range(&mut self, request: Value) -> RS<Value> {
        let request = serde_json::from_value::<JsonRangeRequest>(request)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode json range request error", e))?;
        let response = self
            .inner
            .range_scan(RangeScanRequest::new(
                request.oid.to_oid(),
                json_value_to_universal_bytes(request.start_key)?,
                json_value_to_universal_bytes(request.end_key)?,
            ))
            .await?;
        let items = response
            .into_items()
            .into_iter()
            .map(key_value_to_json)
            .collect::<RS<Vec<_>>>()?;
        Ok(Value::Array(items))
    }

    /// Invoke a stored procedure from a JSON request body.
    pub async fn invoke(&mut self, request: Value) -> RS<Value> {
        let request = serde_json::from_value::<JsonInvokeRequest>(request)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode json invoke request error", e))?;
        let response = self
            .inner
            .invoke_procedure(ProcedureInvokeRequest::new(
                request.session_id,
                request.procedure_name,
                decode_json_bytes(request.procedure_parameters)?,
            ))
            .await?;
        encode_json_bytes(&response.into_result())
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CommandKind {
    Query,
    Execute,
}

#[derive(Debug, Deserialize)]
struct JsonCommandRequest {
    app_name: String,
    #[serde(alias = "command")]
    sql: String,
    #[serde(default)]
    kind: Option<CommandKind>,
}

#[derive(Debug, Deserialize)]
struct JsonGetRequest {
    oid: UniOid,
    key: Value,
}

#[derive(Debug, Deserialize)]
struct JsonPutRequest {
    oid: UniOid,
    key: Value,
    value: Value,
}

#[derive(Debug, Deserialize)]
struct JsonRangeRequest {
    oid: UniOid,
    start_key: Value,
    end_key: Value,
}

#[derive(Debug, Deserialize)]
struct JsonInvokeRequest {
    #[serde(deserialize_with = "deserialize_u128_session_id")]
    session_id: u128,
    procedure_name: String,
    procedure_parameters: Value,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum JsonSessionId {
    Number(u64),
    String(String),
}

fn deserialize_u128_session_id<'de, D>(deserializer: D) -> Result<u128, D::Error>
where
    D: Deserializer<'de>,
{
    match JsonSessionId::deserialize(deserializer)? {
        JsonSessionId::Number(value) => Ok(value as u128),
        JsonSessionId::String(value) => value.parse::<u128>().map_err(de::Error::custom),
    }
}

fn json_value_to_uni_data_value(value: Value) -> RS<UniDataValue> {
    match value {
        Value::Null => Ok(UniDataValue::from_binary(
            serde_json::to_vec(&Value::Null)
                .map_err(|e| mudu_error!(ErrorCode::Encode, "encode null payload error", e))?,
        )),
        Value::Bool(inner) => Ok(UniDataValue::from_scalar(UniScalarValue::from_bool(inner))),
        Value::Number(inner) => {
            if let Some(value) = inner.as_i64() {
                Ok(UniDataValue::from_scalar(UniScalarValue::from_i64(value)))
            } else if let Some(value) = inner.as_u64() {
                Ok(UniDataValue::from_scalar(UniScalarValue::from_u64(value)))
            } else if let Some(value) = inner.as_f64() {
                Ok(UniDataValue::from_scalar(UniScalarValue::from_f64(value)))
            } else {
                Err(mudu_error!(
                    ErrorCode::Decode,
                    "unsupported numeric json payload"
                ))
            }
        }
        Value::String(inner) => Ok(UniDataValue::from_scalar(UniScalarValue::from_string(
            inner,
        ))),
        Value::Array(inner) => inner
            .into_iter()
            .map(json_value_to_uni_data_value)
            .collect::<RS<Vec<_>>>()
            .map(UniDataValue::from_array),
        Value::Object(mut object) => {
            if object.len() == 1 && object.contains_key("base64") {
                let encoded = object
                    .remove("base64")
                    .and_then(|value| value.as_str().map(ToOwned::to_owned))
                    .ok_or_else(|| {
                        mudu_error!(ErrorCode::Decode, "base64 payload must be a string")
                    })?;
                return base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map(UniDataValue::from_binary)
                    .map_err(|e| mudu_error!(ErrorCode::Decode, "decode base64 payload error", e));
            }
            serde_json::to_vec(&Value::Object(object))
                .map(UniDataValue::from_binary)
                .map_err(|e| mudu_error!(ErrorCode::Encode, "encode json object payload error", e))
        }
    }
}

fn json_value_to_universal_bytes(value: Value) -> RS<Vec<u8>> {
    serde_json::to_vec(&json_value_to_uni_data_value(value)?)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode universal kv value error", e))
}

fn decode_uni_data_value(bytes: &[u8]) -> RS<UniDataValue> {
    serde_json::from_slice(bytes)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode universal kv value error", e))
}

fn uni_data_value_to_json_value(value: UniDataValue) -> RS<Value> {
    match value {
        UniDataValue::Scalar(inner) => {
            match inner {
                UniScalarValue::Bool(v) => Ok(Value::Bool(v)),
                UniScalarValue::U8(v) => Ok(json!(v)),
                UniScalarValue::I8(v) => Ok(json!(v)),
                UniScalarValue::U16(v) => Ok(json!(v)),
                UniScalarValue::I16(v) => Ok(json!(v)),
                UniScalarValue::U32(v) => Ok(json!(v)),
                UniScalarValue::I32(v) => Ok(json!(v)),
                UniScalarValue::U64(v) => Ok(json!(v)),
                UniScalarValue::U128(v) => {
                    let bytes: [u8; 16] = v.as_slice().try_into().map_err(|_| {
                        mudu_error!(ErrorCode::Decode, "u128 payload must be 16 bytes")
                    })?;
                    Ok(Value::String(u128::from_be_bytes(bytes).to_string()))
                }
                UniScalarValue::I64(v) => Ok(json!(v)),
                UniScalarValue::I128(v) => {
                    let bytes: [u8; 16] = v.as_slice().try_into().map_err(|_| {
                        mudu_error!(ErrorCode::Decode, "i128 payload must be 16 bytes")
                    })?;
                    Ok(Value::String(i128::from_be_bytes(bytes).to_string()))
                }
                UniScalarValue::F32(v) => Ok(json!(v)),
                UniScalarValue::F64(v) => Ok(json!(v)),
                UniScalarValue::Char(v) => Ok(json!(v.to_string())),
                UniScalarValue::String(v) => Ok(Value::String(v)),
                UniScalarValue::Blob(v) => encode_json_bytes(&v),
                UniScalarValue::Numeric(v) => Ok(Value::String(v)),
                UniScalarValue::Date(v) => Ok(Value::String(v)),
                UniScalarValue::Time(v) => Ok(Value::String(v)),
                UniScalarValue::Timestamp(v) => Ok(Value::String(v)),
                UniScalarValue::TimestampTz(v) => Ok(Value::String(v)),
            }
        }
        UniDataValue::Array(items) => items
            .into_iter()
            .map(uni_data_value_to_json_value)
            .collect::<RS<Vec<_>>>()
            .map(Value::Array),
        UniDataValue::Record(fields) => fields
            .into_iter()
            .map(|f| uni_data_value_to_json_value(f.field_value))
            .collect::<RS<Vec<_>>>()
            .map(Value::Array),
        UniDataValue::Binary(bytes) => encode_json_bytes(&bytes),
    }
}

fn universal_bytes_to_json_value(bytes: &[u8]) -> RS<Value> {
    uni_data_value_to_json_value(decode_uni_data_value(bytes)?)
}

fn decode_json_bytes(value: Value) -> RS<Vec<u8>> {
    if let Value::Object(mut object) = value {
        if object.len() == 1 && object.contains_key("base64") {
            let encoded = object
                .remove("base64")
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
                .ok_or_else(|| mudu_error!(ErrorCode::Decode, "base64 payload must be a string"))?;
            return base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|e| mudu_error!(ErrorCode::Decode, "decode base64 payload error", e));
        }
        return serde_json::to_vec(&Value::Object(object))
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode json payload error", e));
    }
    serde_json::to_vec(&value)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode json payload error", e))
}

fn encode_json_bytes(bytes: &[u8]) -> RS<Value> {
    match serde_json::from_slice::<Value>(bytes) {
        Ok(value) => Ok(value),
        Err(_) => Ok(json!({
            "base64": base64::engine::general_purpose::STANDARD.encode(bytes)
        })),
    }
}

fn key_value_to_json(key_value: KeyValue) -> RS<Value> {
    Ok(json!({
        "key": universal_bytes_to_json_value(key_value.key())?,
        "value": universal_bytes_to_json_value(key_value.value())?,
    }))
}

fn server_response_to_json(response: &ServerResponse) -> RS<Value> {
    let columns = response
        .row_desc()
        .fields()
        .iter()
        .map(|field| Value::String(field.name().to_string()))
        .collect::<Vec<_>>();

    let rows = response
        .rows()
        .iter()
        .map(|row| {
            let values = row
                .values()
                .iter()
                .zip(response.row_desc().fields().iter())
                .map(|(value, field_desc)| {
                    if value.is_null() {
                        Ok(Value::Null)
                    } else if field_desc.data_type().type_family() == TypeFamily::String {
                        Ok(Value::String(value.expect_string().clone()))
                    } else {
                        value
                            .to_textual(field_desc.data_type())
                            .map(|text| Value::String(text.into()))
                    }
                })
                .collect::<RS<Vec<_>>>()?;
            Ok(Value::Array(values))
        })
        .collect::<RS<Vec<_>>>()?;

    Ok(json!({
        "columns": columns,
        "rows": rows,
        "affected_rows": response.affected_rows(),
        "error": response.error(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::async_client::AsyncClient;
    use async_trait::async_trait;
    use mudu_contract::protocol::{
        GetResponse, KeyValue, ProcedureInvokeResponse, PutResponse, RangeScanResponse,
        ServerResponse, SessionCloseRequest, SessionCloseResponse, SessionCreateRequest,
        SessionCreateResponse,
    };
    use mudu_contract::tuple::datum_desc::DatumDesc;
    use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
    use mudu_contract::tuple::tuple_value::TupleValue;
    use mudu_type::data_type::DataType;
    use mudu_type::data_value::DataValue;
    use mudu_type::type_family::TypeFamily;

    struct MockAsyncIoUringTcpClient {
        last_query: Option<ClientRequest>,
        last_execute: Option<ClientRequest>,
        last_batch: Option<ClientRequest>,
        last_get: Option<GetRequest>,
        last_put: Option<PutRequest>,
        last_range: Option<RangeScanRequest>,
        last_invoke: Option<ProcedureInvokeRequest>,
    }

    impl MockAsyncIoUringTcpClient {
        fn new() -> Self {
            Self {
                last_query: None,
                last_execute: None,
                last_batch: None,
                last_get: None,
                last_put: None,
                last_range: None,
                last_invoke: None,
            }
        }
    }

    #[async_trait]
    impl AsyncClient for MockAsyncIoUringTcpClient {
        async fn query(&mut self, request: ClientRequest) -> RS<ServerResponse> {
            self.last_query = Some(request);
            Ok(ServerResponse::new(
                TupleFieldDesc::new(vec![DatumDesc::new(
                    "value".to_string(),
                    DataType::default_for(TypeFamily::String),
                )]),
                vec![TupleValue::from(vec![DataValue::from_string(
                    "1".to_string(),
                )])],
                0,
                None,
            ))
        }

        async fn execute(&mut self, request: ClientRequest) -> RS<ServerResponse> {
            self.last_execute = Some(request);
            Ok(ServerResponse::new(
                TupleFieldDesc::new(vec![]),
                vec![],
                2,
                None,
            ))
        }

        async fn batch(&mut self, request: ClientRequest) -> RS<ServerResponse> {
            self.last_batch = Some(request);
            Ok(ServerResponse::new(
                TupleFieldDesc::new(vec![]),
                vec![],
                3,
                None,
            ))
        }

        async fn get(&mut self, request: GetRequest) -> RS<GetResponse> {
            self.last_get = Some(request);
            Ok(GetResponse::new(Some(
                json_value_to_universal_bytes(json!("value-1")).unwrap(),
            )))
        }

        async fn put(&mut self, request: PutRequest) -> RS<PutResponse> {
            self.last_put = Some(request);
            Ok(PutResponse::new(true))
        }

        async fn range_scan(&mut self, request: RangeScanRequest) -> RS<RangeScanResponse> {
            self.last_range = Some(request);
            Ok(RangeScanResponse::new(vec![
                KeyValue::new(
                    json_value_to_universal_bytes(json!("a")).unwrap(),
                    json_value_to_universal_bytes(json!({"value": 1})).unwrap(),
                ),
                KeyValue::new(
                    json_value_to_universal_bytes(json!({"base64": "/wA="})).unwrap(),
                    json_value_to_universal_bytes(json!({"base64": "AQI="})).unwrap(),
                ),
            ]))
        }

        async fn invoke_procedure(
            &mut self,
            request: ProcedureInvokeRequest,
        ) -> RS<ProcedureInvokeResponse> {
            self.last_invoke = Some(request);
            Ok(ProcedureInvokeResponse::new(vec![0xff, 0x01]))
        }

        async fn create_session(
            &mut self,
            _request: SessionCreateRequest,
        ) -> RS<SessionCreateResponse> {
            Ok(SessionCreateResponse::new(1))
        }

        async fn close_session(
            &mut self,
            _request: SessionCloseRequest,
        ) -> RS<SessionCloseResponse> {
            Ok(SessionCloseResponse::new(true))
        }
    }

    #[test]
    fn json_client_maps_command_requests() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let mut client = JsonClient::new(MockAsyncIoUringTcpClient::new());
            let response = client
                .command(json!({
                    "app_name": "demo",
                    "command": "select 1"
                }))
                .await
                .unwrap();
            assert_eq!(response["rows"], json!([["1"]]));

            let response = client
                .command(json!({
                    "app_name": "demo",
                    "sql": "delete from t",
                    "kind": "execute"
                }))
                .await
                .unwrap();
            assert_eq!(response["affected_rows"], json!(2));

            let inner = client.into_inner();
            assert_eq!(inner.last_query.unwrap().sql(), "select 1");
            assert_eq!(inner.last_execute.unwrap().sql(), "delete from t");
        })
        .unwrap();
    }

    #[test]
    fn server_response_to_json_renders_null_value() {
        let response = ServerResponse::new(
            TupleFieldDesc::new(vec![DatumDesc::new_nullable(
                "name".to_string(),
                DataType::default_for(TypeFamily::String),
                true,
            )]),
            vec![TupleValue::from(vec![DataValue::null()])],
            0,
            None,
        );

        let value = server_response_to_json(&response).unwrap();
        assert_eq!(value["rows"], json!([[null]]));
    }

    #[test]
    fn json_client_maps_kv_and_invoke_payloads() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let mut client = JsonClient::new(MockAsyncIoUringTcpClient::new());

            let put = client
                .put(json!({
                    "oid": {"h": 0, "l": 7},
                    "key": {"user": "u1"},
                    "value": {"score": 9}
                }))
                .await
                .unwrap();
            assert_eq!(put, json!({"ok": true}));

            let get = client
                .get(json!({
                    "oid": {"h": 0, "l": 7},
                    "key": {"user": "u1"}
                }))
                .await
                .unwrap();
            assert_eq!(get, json!("value-1"));

            let range = client
                .range(json!({
                    "oid": {"h": 0, "l": 7},
                    "start_key": "a",
                    "end_key": "z"
                }))
                .await
                .unwrap();
            assert_eq!(
                range,
                json!([
                    {"key": "a", "value": {"value": 1}},
                    {"key": {"base64": "/wA="}, "value": {"base64": "AQI="}}
                ])
            );

            let invoke = client
                .invoke(json!({
                    "session_id": 7,
                    "procedure_name": "app/mod/proc",
                    "procedure_parameters": {"base64": "cGF5bG9hZA=="}
                }))
                .await
                .unwrap();
            assert_eq!(invoke, json!({"base64": "/wE="}));

            let inner = client.into_inner();
            assert_eq!(
                universal_bytes_to_json_value(inner.last_put.unwrap().key()).unwrap(),
                json!({"user": "u1"})
            );
            assert_eq!(
                universal_bytes_to_json_value(inner.last_get.unwrap().key()).unwrap(),
                json!({"user": "u1"})
            );
            assert_eq!(
                universal_bytes_to_json_value(inner.last_range.unwrap().start_key()).unwrap(),
                json!("a")
            );
            assert_eq!(
                inner.last_invoke.unwrap().procedure_parameters(),
                b"payload"
            );
        })
        .unwrap();
    }

    #[test]
    fn json_client_accepts_large_universal_oid() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let mut client = JsonClient::new(MockAsyncIoUringTcpClient::new());
            let session_id = 312629621299694386177868034580325764009u128;
            client
                .put(json!({
                    "oid": serde_json::to_value(UniOid::from_oid(session_id)).unwrap(),
                    "key": "user-1",
                    "value": "value-1"
                }))
                .await
                .unwrap();
            let inner = client.into_inner();
            assert_eq!(inner.last_put.unwrap().session_id(), session_id);
        })
        .unwrap();
    }
}
