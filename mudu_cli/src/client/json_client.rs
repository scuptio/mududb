use crate::client::async_client::{AsyncClient, AsyncClientImpl};
use base64::Engine;
use mudu_binding::universal::uni_dat_value::UniDatValue;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_binding::universal::uni_primitive_value::UniPrimitiveValue;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::protocol::{
    ClientRequest, GetRequest, KeyValue, ProcedureInvokeRequest, PutRequest, RangeScanRequest,
};
use serde::Deserialize;
use serde::de::{self, Deserializer};
use serde_json::{Value, json};

pub struct JsonClient<C> {
    inner: C,
}

impl<C> JsonClient<C> {
    pub fn new(inner: C) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> C {
        self.inner
    }
}

impl JsonClient<AsyncClientImpl> {
    pub async fn connect(addr: &str) -> RS<Self> {
        Ok(Self::new(AsyncClientImpl::connect(addr).await?))
    }
}

impl<C> JsonClient<C>
where
    C: AsyncClient,
{
    pub async fn command(&mut self, request: Value) -> RS<Value> {
        let request = serde_json::from_value::<JsonCommandRequest>(request)
            .map_err(|e| m_error!(EC::DecodeErr, "decode json command request error", e))?;
        let client_request = ClientRequest::new(request.app_name, request.sql);
        let response = if request.kind == Some(CommandKind::Execute) {
            self.inner.execute(client_request).await?
        } else {
            self.inner.query(client_request).await?
        };
        serde_json::to_value(response)
            .map_err(|e| m_error!(EC::EncodeErr, "encode json command response error", e))
    }

    pub async fn put(&mut self, request: Value) -> RS<Value> {
        let request = serde_json::from_value::<JsonPutRequest>(request)
            .map_err(|e| m_error!(EC::DecodeErr, "decode json put request error", e))?;
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

    pub async fn get(&mut self, request: Value) -> RS<Value> {
        let request = serde_json::from_value::<JsonGetRequest>(request)
            .map_err(|e| m_error!(EC::DecodeErr, "decode json get request error", e))?;
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

    pub async fn range(&mut self, request: Value) -> RS<Value> {
        let request = serde_json::from_value::<JsonRangeRequest>(request)
            .map_err(|e| m_error!(EC::DecodeErr, "decode json range request error", e))?;
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

    pub async fn invoke(&mut self, request: Value) -> RS<Value> {
        let request = serde_json::from_value::<JsonInvokeRequest>(request)
            .map_err(|e| m_error!(EC::DecodeErr, "decode json invoke request error", e))?;
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

fn json_value_to_uni_dat_value(value: Value) -> RS<UniDatValue> {
    match value {
        Value::Null => Ok(UniDatValue::from_binary(
            serde_json::to_vec(&Value::Null)
                .map_err(|e| m_error!(EC::EncodeErr, "encode null payload error", e))?,
        )),
        Value::Bool(inner) => Ok(UniDatValue::from_primitive(UniPrimitiveValue::from_bool(inner))),
        Value::Number(inner) => {
            if let Some(value) = inner.as_i64() {
                Ok(UniDatValue::from_primitive(UniPrimitiveValue::from_i64(value)))
            } else if let Some(value) = inner.as_u64() {
                Ok(UniDatValue::from_primitive(UniPrimitiveValue::from_u64(value)))
            } else if let Some(value) = inner.as_f64() {
                Ok(UniDatValue::from_primitive(UniPrimitiveValue::from_f64(value)))
            } else {
                Err(m_error!(EC::DecodeErr, "unsupported numeric json payload"))
            }
        }
        Value::String(inner) => Ok(UniDatValue::from_primitive(
            UniPrimitiveValue::from_string(inner),
        )),
        Value::Array(inner) => inner
            .into_iter()
            .map(json_value_to_uni_dat_value)
            .collect::<RS<Vec<_>>>()
            .map(UniDatValue::from_array),
        Value::Object(mut object) => {
            if object.len() == 1 && object.contains_key("base64") {
                let encoded = object
                    .remove("base64")
                    .and_then(|value| value.as_str().map(ToOwned::to_owned))
                    .ok_or_else(|| m_error!(EC::DecodeErr, "base64 payload must be a string"))?;
                return base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map(UniDatValue::from_binary)
                    .map_err(|e| m_error!(EC::DecodeErr, "decode base64 payload error", e));
            }
            serde_json::to_vec(&Value::Object(object))
                .map(UniDatValue::from_binary)
                .map_err(|e| m_error!(EC::EncodeErr, "encode json object payload error", e))
        }
    }
}

fn json_value_to_universal_bytes(value: Value) -> RS<Vec<u8>> {
    serde_json::to_vec(&json_value_to_uni_dat_value(value)?)
        .map_err(|e| m_error!(EC::EncodeErr, "encode universal kv value error", e))
}

fn decode_uni_dat_value(bytes: &[u8]) -> RS<UniDatValue> {
    serde_json::from_slice(bytes)
        .map_err(|e| m_error!(EC::DecodeErr, "decode universal kv value error", e))
}

fn uni_dat_value_to_json_value(value: UniDatValue) -> RS<Value> {
    match value {
        UniDatValue::Primitive(inner) => match inner {
            UniPrimitiveValue::Bool(v) => Ok(Value::Bool(v)),
            UniPrimitiveValue::U8(v) => Ok(json!(v)),
            UniPrimitiveValue::I8(v) => Ok(json!(v)),
            UniPrimitiveValue::U16(v) => Ok(json!(v)),
            UniPrimitiveValue::I16(v) => Ok(json!(v)),
            UniPrimitiveValue::U32(v) => Ok(json!(v)),
            UniPrimitiveValue::I32(v) => Ok(json!(v)),
            UniPrimitiveValue::U64(v) => Ok(json!(v)),
            UniPrimitiveValue::I64(v) => Ok(json!(v)),
            UniPrimitiveValue::F32(v) => Ok(json!(v)),
            UniPrimitiveValue::F64(v) => Ok(json!(v)),
            UniPrimitiveValue::Char(v) => Ok(json!(v.to_string())),
            UniPrimitiveValue::String(v) => Ok(Value::String(v)),
        },
        UniDatValue::Array(items) | UniDatValue::Record(items) => items
            .into_iter()
            .map(uni_dat_value_to_json_value)
            .collect::<RS<Vec<_>>>()
            .map(Value::Array),
        UniDatValue::Binary(bytes) => encode_json_bytes(&bytes),
    }
}

fn universal_bytes_to_json_value(bytes: &[u8]) -> RS<Value> {
    uni_dat_value_to_json_value(decode_uni_dat_value(bytes)?)
}

fn decode_json_bytes(value: Value) -> RS<Vec<u8>> {
    if let Value::Object(mut object) = value {
        if object.len() == 1 && object.contains_key("base64") {
            let encoded = object
                .remove("base64")
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
                .ok_or_else(|| m_error!(EC::DecodeErr, "base64 payload must be a string"))?;
            return base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|e| m_error!(EC::DecodeErr, "decode base64 payload error", e));
        }
        return serde_json::to_vec(&Value::Object(object))
            .map_err(|e| m_error!(EC::EncodeErr, "encode json payload error", e));
    }
    serde_json::to_vec(&value).map_err(|e| m_error!(EC::EncodeErr, "encode json payload error", e))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::async_client::AsyncClient;
    use async_trait::async_trait;
    use mudu_contract::protocol::{
        GetResponse, KeyValue, ProcedureInvokeResponse, PutResponse, RangeScanResponse,
        ServerResponse,
        SessionCloseRequest, SessionCloseResponse, SessionCreateRequest, SessionCreateResponse,
    };

    struct MockAsyncIoUringTcpClient {
        last_query: Option<ClientRequest>,
        last_execute: Option<ClientRequest>,
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
                vec!["value".to_string()],
                vec![vec!["1".to_string()]],
                0,
                None,
            ))
        }

        async fn execute(&mut self, request: ClientRequest) -> RS<ServerResponse> {
            self.last_execute = Some(request);
            Ok(ServerResponse::new(vec![], vec![], 2, None))
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

    #[tokio::test]
    async fn json_client_maps_command_requests() {
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
    }

    #[tokio::test]
    async fn json_client_maps_kv_and_invoke_payloads() {
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
    }

    #[tokio::test]
    async fn json_client_accepts_large_universal_oid() {
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
    }
}
