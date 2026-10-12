mod batch;
mod execute;
mod get;
mod handshake;
mod procedure_invoke;
mod put;
mod query;
mod range_scan;
mod session_close;
mod session_create;

#[cfg(test)]
mod handshake_test;

use crate::async_func_task::HandleResult;
use crate::request_ctx::RequestCtx;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::protocol::{Frame, MessageType};

/// A protocol message handler registered with the global dispatcher.
#[async_trait]
pub(crate) trait MessageHandler: Send + Sync {
    /// The message type this handler serves.
    fn message_type(&self) -> MessageType;
    /// Dispatch one frame and return the response or an error.
    async fn handle(&self, ctx: &RequestCtx, frame: &Frame) -> RS<HandleResult>;
}

pub(crate) use batch::BatchHandler;
pub(crate) use execute::ExecuteHandler;
pub(crate) use get::GetHandler;
pub(crate) use handshake::HandshakeHandler;
pub(crate) use procedure_invoke::ProcedureInvokeHandler;
pub(crate) use put::PutHandler;
pub(crate) use query::QueryHandler;
pub(crate) use range_scan::RangeScanHandler;
pub(crate) use session_close::SessionCloseHandler;
pub(crate) use session_create::SessionCreateHandler;
