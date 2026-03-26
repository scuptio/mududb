use crate::backend::incoming_session::IncomingSession;
use crate::backend::session_ctx::SessionCtx;
use mudu::common::result::RS;
use mudu_utils::notifier::Waiter;

use mudu_utils::sync::async_task::{AsyncLocalTask, Task};
use mudu_utils::task::spawn_local_task;
use tokio::sync::mpsc::Receiver;
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
                let _ = spawn_local_task(c.into(), &name, async move {
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
