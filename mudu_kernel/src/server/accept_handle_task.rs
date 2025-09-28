use crate::contract::a_task::ATask;
use crate::server::incoming_session::{IncomingSession, SSPSender};
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu_utils::notifier::Notifier;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

impl AcceptHandleTask {
    pub fn new(
        canceller: Notifier,
        bind_addr: SocketAddr,
        ssp_sender_channel: Vec<SSPSender>,
        wait_recovery: Notifier,
    ) -> Self {
        Self {
            canceller,
            name: "accept_session".to_string(),
            bind_addr,
            wait_recovery,
            ssp_sender_channel,
        }
    }

    async fn server_accept(self) -> RS<()> {
        self.wait_recovery.notified().await;
        let listener = TcpListener::bind(self.bind_addr)
            .await
            .map_err(|_e|
                m_error!(ER::NetErr, "bind address error")
            )?;
        info!("server listen on address {}", self.bind_addr);
        let mut session_id: u64 = 0;

        loop {
            let r = listener.accept().await;
            let incoming = r
                .map_err(
                    |_e| m_error!(ER::NetErr, "network accept error", _e))?;
            info!("accept connection {}", incoming.1);
            let param = IncomingSession::new(incoming.1, incoming.0);
            session_id += 1;
            let index = (session_id as usize) % self.ssp_sender_channel.len();
            let r = self.ssp_sender_channel[index].send(param).await;
            r.map_err(|_e| m_error!(ER::SyncErr, "channel send error", _e))?;
        }
    }
}

pub struct AcceptHandleTask {
    canceller: Notifier,
    name: String,
    bind_addr: SocketAddr,
    ssp_sender_channel: Vec<SSPSender>,
    wait_recovery: Notifier,
}

impl ATask for AcceptHandleTask {
    fn notifier(&self) -> Notifier {
        self.canceller.clone()
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(self) -> RS<()> {
        self.server_accept().await
    }
}
