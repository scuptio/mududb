use crate::sync::unique_inner::UniqueInner;
use mudu::common::result::RS;
use std::sync::Arc;

pub trait STask: Send + Sync {
    fn name(&self) -> String;

    fn run(self) -> RS<()>;
}

pub trait STaskRef: Send + Sync {
    fn name(&self) -> String;

    fn run_once(&self) -> RS<()>;
}

impl<T: STask + 'static> STaskRef for UniqueInner<T> {
    fn name(&self) -> String {
        let r = self.map_inner(|t| t.name());
        let s = r.unwrap_or("CTSTaskRef, ref pointer must be not none".to_string());
        s
    }

    fn run_once(&self) -> RS<()> {
        let t = self.inner_into();
        t.run()
    }
}

pub type SyncTask = Arc<dyn STaskRef>;
