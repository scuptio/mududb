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

#[cfg(test)]
mod tests {
    use super::STaskRef;
    use crate::sync::unique_inner::UniqueInner;
    use mudu::common::result::RS;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    struct DemoTask {
        name: String,
        runs: Arc<AtomicUsize>,
    }

    impl super::STask for DemoTask {
        fn name(&self) -> String {
            self.name.clone()
        }

        fn run(self) -> RS<()> {
            self.runs.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn unique_inner_task_ref_exposes_name_and_runs_once() {
        let runs = Arc::new(AtomicUsize::new(0));
        let task = UniqueInner::new(DemoTask {
            name: "demo".to_string(),
            runs: runs.clone(),
        });

        assert_eq!(task.name(), "demo");
        task.run_once().unwrap();
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }
}
