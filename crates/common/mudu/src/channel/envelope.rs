//! The delivered-message ([`Envelope`]) and to-send ([`OutgoingMessage`])
//! types of the channel protocol.

use super::wire::{
    DeliveryMode, EndpointId, MessageId, MessageKind, RecvFilter, ReplyStatus, RequestId,
    serde_opt_u128, serde_u128,
};
use serde::{Deserialize, Serialize};

/// A delivered message with its routing, correlation, and delivery metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    msg_id: MessageId,
    #[serde(with = "serde_u128")]
    request_id: RequestId,
    attempt: u32,
    #[serde(with = "serde_opt_u128")]
    correlation_id: Option<RequestId>,
    #[serde(with = "serde_u128")]
    src: EndpointId,
    #[serde(with = "serde_u128")]
    dst: EndpointId,
    kind: MessageKind,
    payload: Vec<u8>,
    delivery: DeliveryMode,
    deadline_ms: Option<u64>,
    status: ReplyStatus,
}

/// A message to send, with routing, correlation, and delivery metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutgoingMessage {
    kind: MessageKind,
    payload: Vec<u8>,
    correlation_id: Option<RequestId>,
    delivery: DeliveryMode,
    request_id: RequestId,
    attempt: u32,
    deadline_ms: Option<u64>,
    status: ReplyStatus,
}

impl Envelope {
    /// Creates an envelope with explicit routing and correlation metadata;
    /// idempotency fields default to a zero request id, attempt 0, no
    /// deadline, and [`ReplyStatus::Ok`].
    pub fn new(
        msg_id: MessageId,
        correlation_id: Option<RequestId>,
        src: EndpointId,
        dst: EndpointId,
        kind: MessageKind,
        payload: Vec<u8>,
        delivery: DeliveryMode,
    ) -> Self {
        Self {
            msg_id,
            request_id: 0,
            attempt: 0,
            correlation_id,
            src,
            dst,
            kind,
            payload,
            delivery,
            deadline_ms: None,
            status: ReplyStatus::Ok,
        }
    }

    /// Sets the idempotency key of the logical operation this message
    /// belongs to (stable across retries).
    pub fn with_request_id(mut self, request_id: RequestId) -> Self {
        self.request_id = request_id;
        self
    }

    /// Sets the retry attempt counter of this message.
    pub fn with_attempt(mut self, attempt: u32) -> Self {
        self.attempt = attempt;
        self
    }

    /// Sets the absolute deadline (milliseconds since the Unix epoch on the
    /// channel clock) after which the message may be dropped.
    pub fn with_deadline_ms(mut self, deadline_ms: u64) -> Self {
        self.deadline_ms = Some(deadline_ms);
        self
    }

    /// Sets the reply status (only meaningful on responses).
    pub fn with_status(mut self, status: ReplyStatus) -> Self {
        self.status = status;
        self
    }

    /// The message id.
    pub fn msg_id(&self) -> MessageId {
        self.msg_id
    }

    /// The idempotency key of the logical operation.
    pub fn request_id(&self) -> RequestId {
        self.request_id
    }

    /// The retry attempt counter.
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    /// The id of the request this message replies to, if any.
    pub fn correlation_id(&self) -> Option<RequestId> {
        self.correlation_id
    }

    /// The sender endpoint.
    pub fn src(&self) -> &EndpointId {
        &self.src
    }

    /// The destination endpoint.
    pub fn dst(&self) -> &EndpointId {
        &self.dst
    }

    /// The message kind.
    pub fn kind(&self) -> MessageKind {
        self.kind
    }

    /// The payload bytes.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// A copy of the payload bytes.
    pub fn payload_owned(&self) -> Vec<u8> {
        self.payload.clone()
    }

    /// The delivery mode.
    pub fn delivery(&self) -> DeliveryMode {
        self.delivery
    }

    /// The absolute deadline on the channel clock, if any.
    pub fn deadline_ms(&self) -> Option<u64> {
        self.deadline_ms
    }

    /// The reply status.
    pub fn status(&self) -> ReplyStatus {
        self.status
    }

    /// Whether this envelope has passed its deadline at `now_ms` (same clock
    /// as the deadline: milliseconds since the Unix epoch).
    pub fn is_expired_at(&self, now_ms: u64) -> bool {
        self.deadline_ms.is_some_and(|deadline| now_ms >= deadline)
    }

    /// Whether this envelope satisfies every set field of `filter`.
    pub fn matches(&self, filter: &RecvFilter) -> bool {
        filter.src.as_ref().is_none_or(|src| src == self.src())
            && filter.dst.as_ref().is_none_or(|dst| dst == self.dst())
            && filter.kind.is_none_or(|kind| kind == self.kind())
            && filter
                .correlation_id
                .is_none_or(|correlation_id| Some(correlation_id) == self.correlation_id())
            && filter
                .delivery
                .is_none_or(|delivery| delivery == self.delivery())
    }
}

impl OutgoingMessage {
    /// Creates a fire-and-forget message of the given kind.
    pub fn new(kind: MessageKind, payload: Vec<u8>) -> Self {
        Self {
            kind,
            payload,
            correlation_id: None,
            delivery: DeliveryMode::FireAndForget,
            request_id: 0,
            attempt: 0,
            deadline_ms: None,
            status: ReplyStatus::Ok,
        }
    }

    /// Sets the id of the request this message replies to.
    pub fn with_correlation_id(mut self, correlation_id: RequestId) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Sets the delivery mode.
    pub fn with_delivery(mut self, delivery: DeliveryMode) -> Self {
        self.delivery = delivery;
        self
    }

    /// Sets the idempotency key of the logical operation (stable across
    /// retries).
    pub fn with_request_id(mut self, request_id: RequestId) -> Self {
        self.request_id = request_id;
        self
    }

    /// Sets the retry attempt counter of this message.
    pub fn with_attempt(mut self, attempt: u32) -> Self {
        self.attempt = attempt;
        self
    }

    /// Sets the absolute deadline after which the message may be dropped
    /// instead of delivered.
    pub fn with_deadline_ms(mut self, deadline_ms: u64) -> Self {
        self.deadline_ms = Some(deadline_ms);
        self
    }

    /// Sets the reply status (used to send nack responses).
    pub fn with_status(mut self, status: ReplyStatus) -> Self {
        self.status = status;
        self
    }

    /// The message kind.
    pub fn kind(&self) -> MessageKind {
        self.kind
    }

    /// The payload bytes.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// A copy of the payload bytes.
    pub fn payload_owned(&self) -> Vec<u8> {
        self.payload.clone()
    }

    /// The id of the request this message replies to, if any.
    pub fn correlation_id(&self) -> Option<RequestId> {
        self.correlation_id
    }

    /// The delivery mode.
    pub fn delivery(&self) -> DeliveryMode {
        self.delivery
    }

    /// The idempotency key of the logical operation.
    pub fn request_id(&self) -> RequestId {
        self.request_id
    }

    /// The retry attempt counter.
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    /// The absolute deadline on the channel clock, if any.
    pub fn deadline_ms(&self) -> Option<u64> {
        self.deadline_ms
    }

    /// The reply status.
    pub fn status(&self) -> ReplyStatus {
        self.status
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn envelope_serde_roundtrip_preserves_all_fields() {
        let envelope = Envelope::new(
            7,
            Some(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef),
            0xffff_ffff_ffff_ffff_ffff_ffff_ffff_fffe,
            42,
            MessageKind::User(0x7101),
            b"payload".to_vec(),
            DeliveryMode::Response,
        )
        .with_request_id(0xdead_beef_dead_beef_dead_beef_dead_beef)
        .with_attempt(3)
        .with_deadline_ms(1_800_000_000_000)
        .with_status(ReplyStatus::Nack);
        let json = serde_json::to_string(&envelope).unwrap();
        let decoded: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn envelope_matches_considers_delivery_mode() {
        let envelope = Envelope::new(
            1,
            None,
            2,
            1,
            MessageKind::User(9),
            vec![],
            DeliveryMode::Response,
        );
        let filter = RecvFilter {
            delivery: Some(DeliveryMode::Request),
            ..RecvFilter::default()
        };
        assert!(!envelope.matches(&filter));
        let filter = RecvFilter {
            delivery: Some(DeliveryMode::Response),
            ..RecvFilter::default()
        };
        assert!(envelope.matches(&filter));
    }

    #[test]
    fn is_expired_at_compares_against_the_given_clock() {
        let envelope = Envelope::new(
            1,
            None,
            2,
            1,
            MessageKind::User(9),
            vec![],
            DeliveryMode::Request,
        )
        .with_deadline_ms(100);
        assert!(!envelope.is_expired_at(99));
        assert!(envelope.is_expired_at(100));
        let undeadlined = Envelope::new(
            1,
            None,
            2,
            1,
            MessageKind::User(9),
            vec![],
            DeliveryMode::Request,
        );
        assert!(!undeadlined.is_expired_at(u64::MAX));
    }
}
