//! Wire identity types of the channel protocol: endpoint and message ids,
//! message kinds, delivery and reply metadata, receive filters, and the
//! serde adapters that carry 128-bit ids over binary formats.

use crate::common::id::OID;
use serde::{Deserialize, Serialize};

/// Unique id of a single send, handed out by the transport's `send`.
pub type MessageId = u64;
/// Idempotency key of a logical operation: stable across its retries.
pub type RequestId = u128;
/// Channel endpoint id.
///
/// Identity is single-sourced at thread granularity: a database worker's
/// endpoint id is its worker id, which is also the worker thread's task id;
/// any other thread that hosts an endpoint registers its own thread OID
/// (see `mudu_utils::channel::spawn_endpoint_for_current_thread`). When
/// channels span processes the routing table (not this type) gains the node
/// qualification.
pub type EndpointId = OID;
/// Id of a server instance; scopes the channel endpoint registry.
pub type ServerInstanceId = OID;

/// Delivery semantics requested for an outgoing message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeliveryMode {
    /// No reply is expected.
    FireAndForget,
    /// A reply correlated by request id is expected.
    Request,
    /// The message replies to an earlier request.
    Response,
}

/// Kind of a channel-internal (system) message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SystemMessageKind {
    /// Positive acknowledgement.
    Ack,
    /// Negative acknowledgement.
    Nack,
}

/// Kind of a message: application-defined or channel-internal.
///
/// `User` tags are allocated to subsystems in disjoint `0xYY00` blocks; see
/// the namespace table in [`crate::channel`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageKind {
    /// Application-defined kind, identified by a tag from an allocated
    /// subsystem block.
    User(u16),
    /// Channel-internal message.
    System(SystemMessageKind),
}

/// Outcome of a [`DeliveryMode::Response`] message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReplyStatus {
    /// The request was processed; the payload carries the reply.
    Ok,
    /// The request was rejected (unknown kind, decode failure, or a handler
    /// error); the payload carries a UTF-8 error message.
    Nack,
}

/// Filter selecting the messages a receive matches; unset fields match
/// everything.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RecvFilter {
    /// Match only messages from this endpoint.
    pub src: Option<EndpointId>,
    /// Match only messages addressed to this endpoint.
    pub dst: Option<EndpointId>,
    /// Match only messages of this kind.
    pub kind: Option<MessageKind>,
    /// Match only messages correlated to this request id.
    pub correlation_id: Option<RequestId>,
    /// Match only messages with this delivery mode.
    pub delivery: Option<DeliveryMode>,
}

/// Serde adapter encoding `u128` values as 16 big-endian bytes so binary
/// formats without native 128-bit integers (e.g. MessagePack) can carry
/// endpoint and request ids.
pub mod serde_u128 {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serializes `value` as 16 big-endian bytes.
    pub fn serialize<S: Serializer>(value: &u128, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&value.to_be_bytes())
    }

    /// Deserializes 16 big-endian bytes back into a `u128`.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u128, D::Error> {
        let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
        let array: [u8; 16] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| serde::de::Error::custom("invalid u128 byte length"))?;
        Ok(u128::from_be_bytes(array))
    }
}

/// Serde adapter for optional `u128` values; see [`serde_u128`].
pub mod serde_opt_u128 {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serializes the value like [`crate::channel::wire::serde_u128`], preserving `None`.
    pub fn serialize<S: Serializer>(
        value: &Option<u128>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(value) => serializer.serialize_some(&value.to_be_bytes().to_vec()),
            None => serializer.serialize_none(),
        }
    }

    /// Deserializes a value written by [`serialize`].
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<u128>, D::Error> {
        let bytes: Option<Vec<u8>> = Deserialize::deserialize(deserializer)?;
        bytes
            .map(|bytes| {
                let array: [u8; 16] = bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| serde::de::Error::custom("invalid u128 byte length"))?;
                Ok(u128::from_be_bytes(array))
            })
            .transpose()
    }
}
