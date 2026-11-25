use super::KernelHttpApi;
use ::catalog::contract::queue::QUEUE_PAYLOAD_ATTR_START;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_type::type_family_ext::TypeFamilyExt;
use serde_json::Value;
use xcontract::management_rpc::{ManagementRequest, ManagementResponse};

/// Queue receive endpoint backing `HttpApi::receive_queue`: the native
/// `RECEIVE ... [FOR <consumer>] [WAIT FOREVER]` long-poll the WebSocket
/// gateway loops on to fan events out to clients.
impl KernelHttpApi {
    /// Receives up to one payload row from `queue`, optionally blocking
    /// forever for a message, and decodes each payload column into a JSON
    /// value keyed by its column name (the queue system columns `__seq`/
    /// `__lsn` are stripped).
    pub(super) async fn receive_queue(
        &self,
        queue: &str,
        consumer: Option<&str>,
        wait_forever: bool,
    ) -> RS<Vec<Value>> {
        let desc = self.resolve_queue_by_name(queue).await?;
        let workers = self.live_workers()?;
        let owner_id = self.find_queue_owner(&workers, desc.id()).await?;
        let response = self
            .ask_worker(
                owner_id,
                ManagementRequest::ReceiveQueue {
                    queue_oid: desc.id(),
                    consumer: consumer.map(str::to_string),
                    count: 1,
                    wait_forever,
                },
            )
            .await?;
        let rows = match response {
            ManagementResponse::Received { rows } => rows,
            other => {
                return Err(mudu_error!(
                    ErrorCode::Internal,
                    format!("unexpected management reply for queue receive: {other:?}")
                ));
            }
        };
        let payload_fields = &desc.fields()[QUEUE_PAYLOAD_ATTR_START..];
        let mut decoded = Vec::with_capacity(rows.len());
        for row in rows {
            let mut object = serde_json::Map::new();
            for (field, datum) in payload_fields.iter().zip(row.iter()) {
                let value = match datum {
                    Some(bytes) => {
                        let (value, _) =
                            field.type_desc().type_family().fn_recv()(bytes, field.type_desc())
                                .map_err(|e| e.to_m_err())?;
                        serde_json::to_value(&value).map_err(|e| {
                            mudu_error!(
                                ErrorCode::Decode,
                                format!("encode queue payload column {} as JSON", field.name()),
                                e
                            )
                        })?
                    }
                    None => Value::Null,
                };
                object.insert(field.name().clone(), value);
            }
            decoded.push(Value::Object(object));
        }
        Ok(decoded)
    }
}
