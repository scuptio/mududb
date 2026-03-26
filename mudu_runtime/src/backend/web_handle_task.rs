use mudu::common::result::RS;
use mudu_utils::notifier::{Notifier, Waiter};
use mudu_utils::sync::async_task::{AsyncLocalTask, Task};

use crate::backend::mududb_cfg::MuduDBCfg;
use crate::backend::web_serve;

impl WebHandleTask {
    pub fn new(
        cfg: MuduDBCfg,
        name: String,
        notifier: Waiter,
        opt_db_init_notifier: Option<Notifier>,
    ) -> Self {
        Self {
            cfg,
            name,
            waiter: notifier,
            opt_db_init_notifier,
        }
    }
}

pub struct WebHandleTask {
    cfg: MuduDBCfg,
    name: String,
    waiter: Waiter,
    opt_db_init_notifier: Option<Notifier>,
}

impl Task for WebHandleTask {}

impl AsyncLocalTask for WebHandleTask {
    fn waiter(&self) -> Waiter {
        self.waiter.clone()
    }

    fn name(&self) -> String {
        self.name.clone()
    }
    fn async_run_local(self) -> impl Future<Output = RS<()>> {
        web_serve::async_serve(self.cfg, self.waiter, self.opt_db_init_notifier)
    }
}
