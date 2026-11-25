use crate::backend::incoming_session::IncomingSession;
use crate::backend::session_ctx::SessionCtx;
use mudu::common::result::RS;
use mudu_utils::notifier::Waiter;

use mudu_sys::sync::async_::async_task::{AsyncLocalTask, Task};
use mudu_sys::tokio::sync::mpsc::Receiver;
use mudu_utils::task_async::spawn_local_task;
use tracing::error;

pub struct SessionHandleTask {
    receiver: Vec<Receiver<IncomingSession>>,
    thd_ctx: SessionCtx,
    name: String,
    waiter: Waiter,
}

impl SessionHandleTask {
    pub fn new(db_path: String, receiver: Vec<Receiver<IncomingSession>>, waiter: Waiter) -> Self {
        Self {
            receiver,
            thd_ctx: SessionCtx::new(db_path),
            name: "session handle task".to_string(),
            waiter,
        }
    }

    async fn serve_handle_connect(self) -> RS<()> {
        let receivers = self.receiver;
        let ctx = self.thd_ctx;
        let canceller = self.waiter;
        let name = self.name;
        for (i, receiver) in receivers.into_iter().enumerate() {
            let name_i = format!("{}_{}", name, i);
            handle_one_receiver_connect(canceller.clone(), receiver, ctx.clone(), name_i).await?;
        }
        Ok(())
    }
}

async fn handle_one_receiver_connect(
    waiter: Waiter,
    receiver: Receiver<IncomingSession>,
    ss_ctx: SessionCtx,
    name: String,
) -> RS<()> {
    let mut receiver = receiver;
    loop {
        let r = receiver.recv().await;
        match r {
            Some(p) => {
                let c = waiter.clone();
                let t = ss_ctx.clone();
                let _ = spawn_local_task(c, &name, async move {
                    let r = p.session_handler_task(t).await;
                    match r {
                        Ok(_) => {}
                        Err(e) => {
                            error!("handle session task error {}", e);
                        }
                    }
                });
            }
            None => {
                break;
            }
        };
    }
    Ok(())
}
impl Task for SessionHandleTask {}

impl AsyncLocalTask for SessionHandleTask {
    fn waiter(&self) -> Waiter {
        self.waiter.clone()
    }
    fn name(&self) -> String {
        self.name.clone()
    }

    fn async_run_local(self) -> impl Future<Output = RS<()>> {
        self.serve_handle_connect()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use mudu_sys::net::{AsyncTcpListener, AsyncTcpStream};
    use mudu_utils::notifier::notify_wait;

    #[test]
    fn serve_handle_connect_spawns_session_tasks_until_channel_closes() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let (tx, rx) = mudu_sys::tokio::sync::mpsc::channel(4);
            let (stop_tx, stop_rx) = notify_wait();
            let task = SessionHandleTask::new(
                mudu_sys::env_var::temp_dir()
                    .join(format!("sess_task_{}", mudu_utils::oid::gen_oid()))
                    .to_string_lossy()
                    .to_string(),
                vec![rx],
                stop_rx,
            );

            // One incoming session: a client that connects and drops, so the
            // pgwire session task ends on EOF.
            let listener = AsyncTcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let client = AsyncTcpStream::connect(addr).await.unwrap();
            let (socket, peer_addr) = listener.accept().await.unwrap();
            tx.send(IncomingSession::new(peer_addr, socket))
                .await
                .unwrap();
            drop(client);
            drop(tx); // close the channel so the serve loop exits

            task.serve_handle_connect().await.unwrap();
            // The session task runs detached on this local set; give it one
            // turn to observe the EOF and log-and-forget its error.
            mudu_sys::tokio::task::yield_now().await;
            let _ = stop_tx;
        })
        .unwrap();
    }

    #[test]
    fn serve_handle_connect_with_no_receivers_returns_immediately() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let (_stop_tx, stop_rx) = notify_wait();
            let task = SessionHandleTask::new("/tmp/none".to_string(), vec![], stop_rx);
            task.serve_handle_connect().await.unwrap();
        })
        .unwrap();
    }
}
