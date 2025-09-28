use crate::server::session_mgr::SessionMgr;
use crate::x_engine::thd_ctx::ThdCtx;
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use pgwire::tokio::process_socket;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender};

pub type SSPSender = Sender<IncomingSession>;
pub type SSPReceiver = Receiver<IncomingSession>;

pub struct IncomingSession {
    //wait_recovery_notified:Notifier,
    incoming_addr: SocketAddr,
    tcp_socket: TcpStream,
}

impl IncomingSession {
    pub fn new(incoming_addr: SocketAddr, tcp_socket: TcpStream) -> Self {
        Self {
            incoming_addr,
            tcp_socket,
        }
    }

    pub async fn session_handler_task(self, ctx: ThdCtx) -> RS<()> {
        let session_mgr = SessionMgr::new(ctx);
        let r = process_socket(self.tcp_socket, None, session_mgr).await;
        r.map_err(|e| {
            m_error!(ER::NetErr, "PG Wire handle error", e)
        })?;
        Ok(())
    }
}
