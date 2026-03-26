use crate::backend::incoming_session::{IncomingSession, SSPSender};
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu_utils::notifier::Waiter;
use mudu_utils::sync::async_task::{AsyncLocalTask, Task};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

impl AcceptHandleTask {
    pub fn new(
        canceller: Waiter,
        bind_addr: SocketAddr,
        ssp_sender_channel: Vec<SSPSender>,
        wait_recovery: Waiter,
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
        self.wait_recovery.wait().await;
        let listener = TcpListener::bind(self.bind_addr)
            .await
            .map_err(|_e| m_error!(ER::NetErr, "bind address error"))?;
        info!("server listen on address {}", self.bind_addr);
        let mut session_id: u64 = 0;

        loop {
            let r = listener.accept().await;
            let incoming = r.map_err(|_e| m_error!(ER::NetErr, "client accept error", _e))?;
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
    canceller: Waiter,
    name: String,
    bind_addr: SocketAddr,
    ssp_sender_channel: Vec<SSPSender>,
    wait_recovery: Waiter,
}

impl Task for AcceptHandleTask {}

#[async_trait]
impl AsyncLocalTask for AcceptHandleTask {
    fn waiter(&self) -> Waiter {
        self.canceller.clone()
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn async_run_local(self) -> impl Future<Output = RS<()>> {
        self.server_accept()
    }
}
