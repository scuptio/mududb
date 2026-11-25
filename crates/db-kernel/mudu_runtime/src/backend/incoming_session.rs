use crate::backend::session_ctx::SessionCtx;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_sys::net::AsyncTcpStream;
use mudu_sys::tokio::sync::mpsc::Sender;
use pgwire::tokio::process_socket;
use std::net::SocketAddr;

pub type SSPSender = Sender<IncomingSession>;

pub struct IncomingSession {
    //wait_recovery_notified:Notifier,
    _incoming_addr: SocketAddr,
    tcp_socket: AsyncTcpStream,
}

impl IncomingSession {
    pub fn new(incoming_addr: SocketAddr, tcp_socket: AsyncTcpStream) -> Self {
        Self {
            _incoming_addr: incoming_addr,
            tcp_socket,
        }
    }

    pub async fn session_handler_task(self, ctx: SessionCtx) -> RS<()> {
        let r = process_socket(self.tcp_socket.into_inner(), None, ctx).await;
        r.map_err(|e| mudu_error!(ER::Network, "PG Wire handle error", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::backend::session_ctx::SessionCtx;
    use mudu_sys::net::AsyncTcpListener;

    #[test]
    fn session_handler_task_surfaces_client_disconnect() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let listener = AsyncTcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            // A client that connects and immediately drops the connection:
            // the pgwire startup read hits EOF inside process_socket.
            let client = AsyncTcpStream::connect(addr).await.unwrap();
            let (socket, peer_addr) = listener.accept().await.unwrap();
            drop(client);

            let session = IncomingSession::new(peer_addr, socket);
            let ctx = SessionCtx::new(
                mudu_sys::env_var::temp_dir()
                    .join(format!("incoming_session_{}", mudu_utils::oid::gen_oid()))
                    .to_string_lossy()
                    .to_string(),
            );
            // EOF mid-startup surfaces as an error (or a clean close);
            // either way the task returns instead of hanging.
            let _ = session.session_handler_task(ctx).await;
        })
        .unwrap();
    }
}
