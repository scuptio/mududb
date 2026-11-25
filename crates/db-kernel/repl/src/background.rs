//! Shared local executor for business replay, separate from the Raft owner.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::sync_::channel::{sync_channel, SyncSender};
use mudu_sys::sync::sync_::unbounded_channel::SyncReceiver;
use mudu_sys::task::async_::{spawn_local_detached, CurrentThreadTaskRuntime};
use mudu_sys::task::sync::spawn_thread_named;
use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;
use tokio::sync::mpsc;

struct Job {
    name: &'static str,
    future: Pin<Box<dyn Future<Output = RS<()>> + Send>>,
    done: SyncSender<RS<()>>,
}

pub(crate) struct BackgroundTask {
    done: SyncReceiver<RS<()>>,
}

impl BackgroundTask {
    pub(crate) fn join(self) -> RS<()> {
        self.done.recv()?
    }
}

fn start_executor() -> RS<mpsc::Sender<Job>> {
    let (sender, mut receiver) = mpsc::channel::<Job>(256);
    let (ready, started) = sync_channel(1);
    spawn_thread_named("mudu-replay", move || {
        // Construct and poll the LocalSet on its owner thread. Business replay
        // opens storage handles whose mailboxes spawn thread-confined tasks.
        let runtime = match CurrentThreadTaskRuntime::with_max_blocking_threads(4) {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = ready.try_send(Err(error));
                return;
            }
        };
        if ready.try_send(Ok(())).is_err() {
            return;
        }
        runtime.block_on(async move {
            while let Some(Job { name, future, done }) = receiver.recv().await {
                let task = spawn_local_detached(name, future);
                let completion = done.clone();
                // Join outside the replay task so a panic is reported to stop(),
                // rather than losing the completion sender during unwinding.
                if let Err(error) = spawn_local_detached("replay-completion", async move {
                    let result = match task {
                        Ok(task) => match task.await {
                            Ok(Some(result)) => result,
                            Ok(None) => {
                                Err(mudu_error!(ErrorCode::Tokio, format!("{name} cancelled")))
                            }
                            Err(error) => Err(mudu_error!(
                                ErrorCode::Tokio,
                                format!("{name} failed"),
                                error
                            )),
                        },
                        Err(error) => Err(error),
                    };
                    if let Err(error) = &result {
                        tracing::error!(name, %error, "replication task failed");
                    }
                    let _ = completion.try_send(result);
                }) {
                    let _ = done.try_send(Err(error));
                }
            }
        });
    })?;
    started.recv()??;
    Ok(sender)
}

pub(crate) fn spawn(
    name: &'static str,
    future: impl Future<Output = RS<()>> + Send + 'static,
) -> RS<BackgroundTask> {
    static EXECUTOR: OnceLock<Result<mpsc::Sender<Job>, String>> = OnceLock::new();
    let executor = EXECUTOR
        .get_or_init(|| start_executor().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| mudu_error!(ErrorCode::Tokio, e.clone()))?;
    let (done, wait) = sync_channel(1);
    executor
        .try_send(Job {
            name,
            future: Box::pin(future),
            done,
        })
        .map_err(|error| {
            mudu_error!(
                ErrorCode::Synchronization,
                format!("replay admission failed: {error}")
            )
        })?;
    Ok(BackgroundTask { done: wait })
}
