use crate::backend::session_ctx::SessionCtx;
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu_sys::tokio::net::TcpStream;
use mudu_sys::tokio::sync::mpsc::Sender;
use pgwire::tokio::process_socket;
use std::net::SocketAddr;

pub type SSPSender = Sender<IncomingSession>;

pub struct IncomingSession {
    //wait_recovery_notified:Notifier,
    _incoming_addr: SocketAddr,
    tcp_socket: TcpStream,
}

impl IncomingSession {
    pub fn new(incoming_addr: SocketAddr, tcp_socket: TcpStream) -> Self {
        Self {
            _incoming_addr: incoming_addr,
            tcp_socket,
        }
    }

    pub async fn session_handler_task(self, ctx: SessionCtx) -> RS<()> {
        let r = process_socket(self.tcp_socket, None, ctx).await;
        r.map_err(|e| m_error!(ER::NetErr, "PG Wire handle error", e))?;
        Ok(())
    }
}
