use crate::contract::async_listener::AsyncListener;
use crate::contract::async_stream::AsyncStream;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::sync::Arc;

/// Async network operations abstraction.
#[async_trait]
pub trait AsyncNet: Send + Sync {
    /// Bind a TCP listener to `addr`.
    async fn bind_tcp(&self, _addr: std::net::SocketAddr) -> RS<Arc<dyn AsyncListener>> {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "async net bind_tcp is not implemented"
        ))
    }

    /// Open a TCP connection to `addr`.
    async fn connect_tcp(&self, _addr: std::net::SocketAddr) -> RS<Box<dyn AsyncStream>> {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "async net connect_tcp is not implemented"
        ))
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::*;
    use mudu::error::ErrorCode;
    use std::net::SocketAddr;

    struct MockNet;

    #[async_trait]
    impl AsyncNet for MockNet {}

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        let waker = std::task::Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future: Pin<Box<F>> = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    #[test]
    fn bind_tcp_default_returns_not_implemented() {
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        match block_on(MockNet.bind_tcp(addr)) {
            Err(err) => assert_eq!(err.ec(), ErrorCode::NotImplemented),
            Ok(_) => panic!("expected NotImplemented error"),
        }
    }

    #[test]
    fn connect_tcp_default_returns_not_implemented() {
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        match block_on(MockNet.connect_tcp(addr)) {
            Err(err) => assert_eq!(err.ec(), ErrorCode::NotImplemented),
            Ok(_) => panic!("expected NotImplemented error"),
        }
    }
}
