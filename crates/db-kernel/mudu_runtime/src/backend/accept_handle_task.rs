use crate::backend::incoming_session::{IncomingSession, SSPSender};
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_sys::net::AsyncTcpListener;
use mudu_sys::sync::async_::async_task::{AsyncLocalTask, Task};
use mudu_utils::notifier::Waiter;
use std::net::SocketAddr;
use tracing::{debug, info};

#[cfg(test)]
use mudu_sys::net::sync::StdTcpListener;

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
        let listener = AsyncTcpListener::bind(self.bind_addr)
            .await
            .map_err(|_e| mudu_error!(ER::Network, "bind address error"))?;
        info!("server listen on address {}", self.bind_addr);
        let mut session_id: u64 = 0;

        loop {
            let r = listener.accept().await;
            let incoming = r.map_err(|_e| mudu_error!(ER::Network, "client accept error", _e))?;
            debug!("accept connection {}", incoming.1);

            let param = IncomingSession::new(incoming.1, incoming.0);
            session_id += 1;
            let index = (session_id as usize) % self.ssp_sender_channel.len();
            let r = self.ssp_sender_channel[index].send(param).await;
            r.map_err(|_e| mudu_error!(ER::Synchronization, "channel send error", _e))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn reserve_local_addr() -> SocketAddr {
        let listener = StdTcpListener::bind("127.0.0.1:0".parse::<SocketAddr>().unwrap()).unwrap();
        listener.local_addr().unwrap()
    }

    #[tokio::test]
    async fn constructor_and_trait_getters() {
        let (cancel_notifier, cancel_waiter) = mudu_utils::notifier::notify_wait();
        let (_recovery_notifier, recovery_waiter) = mudu_utils::notifier::notify_wait();
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let task = AcceptHandleTask::new(
            cancel_waiter.clone(),
            "127.0.0.1:0".parse().unwrap(),
            vec![tx],
            recovery_waiter,
        );
        assert_eq!(task.name(), "accept_session");
        cancel_notifier.notify_all();
        mudu_sys::timeout(Duration::from_secs(1), task.waiter().wait())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn server_accept_fails_when_address_in_use() {
        let holder = StdTcpListener::bind("127.0.0.1:0".parse::<SocketAddr>().unwrap()).unwrap();
        let addr = holder.local_addr().unwrap();
        let (_cancel_notifier, cancel_waiter) = mudu_utils::notifier::notify_wait();
        let (recovery_notifier, recovery_waiter) = mudu_utils::notifier::notify_wait();
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let task = AcceptHandleTask::new(cancel_waiter, addr, vec![tx], recovery_waiter);
        recovery_notifier.notify_all();
        let result = task.server_accept().await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("bind address error")
        );
    }

    #[tokio::test]
    async fn server_accept_waits_for_recovery_signal() {
        let (_cancel_notifier, cancel_waiter) = mudu_utils::notifier::notify_wait();
        let (_recovery_notifier, recovery_waiter) = mudu_utils::notifier::notify_wait();
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let task = AcceptHandleTask::new(
            cancel_waiter,
            "127.0.0.1:0".parse().unwrap(),
            vec![tx],
            recovery_waiter,
        );
        let result = mudu_sys::timeout(Duration::from_millis(100), task.server_accept()).await;
        assert!(
            result.is_none(),
            "server_accept should wait for recovery signal"
        );
    }

    #[tokio::test]
    async fn server_accept_dispatches_one_incoming_session() {
        let addr = reserve_local_addr();
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let (_cancel_notifier, cancel_waiter) = mudu_utils::notifier::notify_wait();
        let (recovery_notifier, recovery_waiter) = mudu_utils::notifier::notify_wait();
        let task = AcceptHandleTask::new(cancel_waiter, addr, vec![tx], recovery_waiter);
        recovery_notifier.notify_all();

        let accept_fut = task.server_accept();
        tokio::pin!(accept_fut);
        tokio::select! {
            biased;
            _ = &mut accept_fut => panic!("server_accept should not finish"),
            _ = async {
                let _ = mudu_sys::net::AsyncTcpStream::connect(addr).await.unwrap();
                mudu_sys::sleep(Duration::from_millis(100)).await.unwrap();
            } => {}
        }

        let _session = mudu_sys::timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .expect("session should be dispatched");
    }

    #[tokio::test]
    async fn server_accept_round_robins_across_two_senders() {
        let addr = reserve_local_addr();
        let (tx0, mut rx0) = tokio::sync::mpsc::channel(1);
        let (tx1, mut rx1) = tokio::sync::mpsc::channel(1);
        let (_cancel_notifier, cancel_waiter) = mudu_utils::notifier::notify_wait();
        let (recovery_notifier, recovery_waiter) = mudu_utils::notifier::notify_wait();
        let task = AcceptHandleTask::new(cancel_waiter, addr, vec![tx0, tx1], recovery_waiter);
        recovery_notifier.notify_all();

        let accept_fut = task.server_accept();
        tokio::pin!(accept_fut);
        tokio::select! {
            biased;
            _ = &mut accept_fut => panic!("server_accept should not finish"),
            _ = async {
                let _ = mudu_sys::net::AsyncTcpStream::connect(addr).await.unwrap();
                let _ = mudu_sys::net::AsyncTcpStream::connect(addr).await.unwrap();
                mudu_sys::sleep(Duration::from_millis(100)).await.unwrap();
            } => {}
        }

        let _first = mudu_sys::timeout(Duration::from_secs(1), rx1.recv())
            .await
            .unwrap()
            .expect("first session to channel 1");
        let _second = mudu_sys::timeout(Duration::from_secs(1), rx0.recv())
            .await
            .unwrap()
            .expect("second session to channel 0");
    }
}
