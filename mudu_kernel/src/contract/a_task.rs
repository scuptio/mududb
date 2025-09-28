use crate::sync::unique_inner::UniqueInner;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_utils::notifier::Notifier;
use mudu_utils::task::spawn_local_task;
use std::sync::Arc;

// A-synchronized task run in local thread
pub trait ATask {
    fn notifier(&self) -> Notifier;

    fn name(&self) -> String;

    async fn run(self) -> RS<()>;
}

#[async_trait]
pub trait ATaskRef: Send + Sync {
    fn name(&self) -> String;

    fn notifier(&self) -> Notifier;

    async fn run_once(&self) -> RS<()>;
}

#[async_trait]
impl<T: ATask + 'static> ATaskRef for UniqueInner<T> {
    fn name(&self) -> String {
        let r = self.map_inner(|t| t.name());
        let s = r.expect("run once can only invoke once");
        s
    }

    fn notifier(&self) -> Notifier {
        let r = self.map_inner(|t| t.notifier());
        if let Some(n) = r {
            n
        } else {
            panic!("run once can only invoke once")
        }
    }

    async fn run_once(&self) -> RS<()> {
        let t = self.inner_into();
        let _ = spawn_local_task(
            t.notifier(),
            t.name().as_str(),
            async move { t.run().await },
        );
        Ok(())
    }
}

pub type AsyncTask = Arc<dyn ATaskRef>;
