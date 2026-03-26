use crate::server::incoming_session::SSPReceiver;
use crate::x_engine::thd_ctx::ThdCtx;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_utils::notifier::NotifyWait;
use mudu_utils::sync::a_task::ATask;
use mudu_utils::task::spawn_local_task;
use tracing::error;

pub struct SessionHandleTask {
    thd_ctx: ThdCtx,
    name: String,
    canceller: NotifyWait,
    receiver: SSPReceiver,
}

impl SessionHandleTask {
    pub fn new(
        thd_ctx: ThdCtx,
        name: String,
        receiver: SSPReceiver,
        canceller: NotifyWait,
    ) -> Self {
        Self {
            thd_ctx,
            name,
            canceller,
            receiver,
        }
    }

    async fn serve_handle_connect(self) -> RS<()> {
        let mut receiver = self.receiver;
        let canceller = self.canceller;
        loop {
            let r = receiver.recv().await;
            match r {
                Some(p) => {
                    let c = canceller.clone();
                    let t = self.thd_ctx.clone();
                    let _ = spawn_local_task(c, "", async move {
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
}
#[async_trait]
impl ATask for SessionHandleTask {
    fn notifier(&self) -> NotifyWait {
        self.canceller.clone()
    }
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(self) -> RS<()> {
        self.serve_handle_connect().await
    }
}
