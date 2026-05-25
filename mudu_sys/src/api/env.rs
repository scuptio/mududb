use crate::api::fs::SysFs;
use crate::api::net::SysNet;
use crate::api::random::SysRandom;
use crate::api::sync::SysSync;
use crate::api::task_async::SysTaskAsync;
use crate::api::task_sync::SysTaskSync;
use crate::api::time::SysTime;

pub trait SysEnv: Send + Sync {
    fn time(&self) -> &dyn SysTime;
    fn random(&self) -> &dyn SysRandom;
    fn fs(&self) -> &dyn SysFs;
    fn net(&self) -> &dyn SysNet;
    fn task_async(&self) -> &dyn SysTaskAsync;
    fn task_sync(&self) -> &dyn SysTaskSync;
    fn sync(&self) -> &dyn SysSync;
}
