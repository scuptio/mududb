//! Typed-message traits of the channel protocol: a message type declares
//! its wire [`MessageKind`], reply type, idempotency, and codec once
//! ([`ChannelMessage`], usually via
//! [`impl_channel_message!`](crate::impl_channel_message)) and the runtime
//! layer (`mudu_utils::channel`) builds ask/tell and typed services on top.

use super::wire::{EndpointId, MessageKind, RequestId};
use crate::common::result::RS;

/// The reply half of a [`ChannelMessage`]: how it encodes onto the wire.
///
/// `()` is the reply of fire-and-forget messages and makes the message
/// tell-only.
pub trait ChannelReply: Send + 'static {
    /// Encodes the reply into payload bytes.
    fn encode(&self) -> RS<Vec<u8>>;
    /// Decodes the reply from payload bytes.
    fn decode(bytes: &[u8]) -> RS<Self>
    where
        Self: Sized;
}

impl ChannelReply for () {
    fn encode(&self) -> RS<Vec<u8>> {
        Ok(Vec::new())
    }

    fn decode(_bytes: &[u8]) -> RS<Self> {
        Ok(())
    }
}

/// A typed channel message: the unit of the typed API.
///
/// `KIND` is the message's wire identity, allocated per the namespace table
/// in [`crate::channel`]; requests and responses share it and are told apart
/// by delivery mode.
pub trait ChannelMessage: Send + 'static {
    /// The reply type of this message; `()` makes it tell-only.
    type Reply: ChannelReply;
    /// The wire kind of this message type.
    const KIND: MessageKind;
    /// Whether replaying this message with the same request id is safe;
    /// callers use it to decide whether a timed-out ask may be retried.
    const IDEMPOTENT: bool = false;
    /// Encodes the message into payload bytes.
    fn encode(&self) -> RS<Vec<u8>>;
    /// Decodes the message from payload bytes.
    fn decode(bytes: &[u8]) -> RS<Self>
    where
        Self: Sized;
}

/// Call context handed to service handlers: who sent the request and under
/// which idempotency key (for receiver-side dedup).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceCtx {
    src: EndpointId,
    request_id: RequestId,
    attempt: u32,
}

impl ServiceCtx {
    /// Creates a context explicitly; production contexts come from the
    /// dispatch path, this constructor exists for handler tests.
    pub fn new(src: EndpointId, request_id: RequestId, attempt: u32) -> Self {
        Self {
            src,
            request_id,
            attempt,
        }
    }

    /// The endpoint the request came from.
    pub fn src(&self) -> EndpointId {
        self.src
    }

    /// The idempotency key of the request (stable across its retries).
    pub fn request_id(&self) -> RequestId {
        self.request_id
    }

    /// The retry attempt counter of the request.
    pub fn attempt(&self) -> u32 {
        self.attempt
    }
}

/// Implements [`ChannelMessage`] and [`ChannelReply`] for serde types using
/// MessagePack (`rmp-serde`) as the wire codec.
///
/// The invoking crate must depend on `rmp-serde` and `mudu`. Use the
/// `tell_only` form for fire-and-forget messages (reply type `()`).
#[macro_export]
macro_rules! impl_channel_message {
    ($message:ty, tell_only, kind = $kind:expr) => {
        impl $crate::channel::message::ChannelMessage for $message {
            type Reply = ();
            const KIND: $crate::channel::wire::MessageKind = $kind;

            fn encode(&self) -> $crate::common::result::RS<::std::vec::Vec<u8>> {
                ::rmp_serde::to_vec(self).map_err(|e| {
                    $crate::mudu_error!(
                        $crate::error::ErrorCode::Encode,
                        "encode channel message error",
                        e
                    )
                })
            }

            fn decode(bytes: &[u8]) -> $crate::common::result::RS<Self> {
                ::rmp_serde::from_slice(bytes).map_err(|e| {
                    $crate::mudu_error!(
                        $crate::error::ErrorCode::Decode,
                        "decode channel message error",
                        e
                    )
                })
            }
        }
    };
    ($message:ty, reply = $reply:ty, kind = $kind:expr $(, idempotent = $idempotent:expr)?) => {
        impl $crate::channel::message::ChannelMessage for $message {
            type Reply = $reply;
            const KIND: $crate::channel::wire::MessageKind = $kind;
            $(const IDEMPOTENT: bool = $idempotent;)?

            fn encode(&self) -> $crate::common::result::RS<::std::vec::Vec<u8>> {
                ::rmp_serde::to_vec(self).map_err(|e| {
                    $crate::mudu_error!(
                        $crate::error::ErrorCode::Encode,
                        "encode channel message error",
                        e
                    )
                })
            }

            fn decode(bytes: &[u8]) -> $crate::common::result::RS<Self> {
                ::rmp_serde::from_slice(bytes).map_err(|e| {
                    $crate::mudu_error!(
                        $crate::error::ErrorCode::Decode,
                        "decode channel message error",
                        e
                    )
                })
            }
        }

        impl $crate::channel::message::ChannelReply for $reply {
            fn encode(&self) -> $crate::common::result::RS<::std::vec::Vec<u8>> {
                ::rmp_serde::to_vec(self).map_err(|e| {
                    $crate::mudu_error!(
                        $crate::error::ErrorCode::Encode,
                        "encode channel reply error",
                        e
                    )
                })
            }

            fn decode(bytes: &[u8]) -> $crate::common::result::RS<Self> {
                ::rmp_serde::from_slice(bytes).map_err(|e| {
                    $crate::mudu_error!(
                        $crate::error::ErrorCode::Decode,
                        "decode channel reply error",
                        e
                    )
                })
            }
        }
    };
}
