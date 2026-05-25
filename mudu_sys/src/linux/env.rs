use crate::api::env::SysEnv;
use crate::api::fs::SysFs;
use crate::api::net::SysNet;
use crate::api::random::SysRandom;
use crate::api::sync::SysSync;
use crate::api::task_async::SysTaskAsync;
use crate::api::task_sync::SysTaskSync;
use crate::api::time::SysTime;
use crate::linux::fs::LinuxFs;
use crate::linux::net::LinuxNet;
use crate::linux::random::LinuxRandom;
use crate::linux::sync::LinuxSync;
use crate::linux::task_async::LinuxTaskAsync;
use crate::linux::task_sync::LinuxTaskSync;
use crate::linux::time::LinuxTime;

pub struct LinuxSysEnv {
    time: LinuxTime,
    random: LinuxRandom,
    fs: LinuxFs,
    net: LinuxNet,
    task_async: LinuxTaskAsync,
    task_sync: LinuxTaskSync,
    sync: LinuxSync,
}

impl LinuxSysEnv {
    pub fn new() -> Self {
        Self {
            time: LinuxTime,
            random: LinuxRandom,
            fs: LinuxFs,
            net: LinuxNet,
            task_async: LinuxTaskAsync,
            task_sync: LinuxTaskSync,
            sync: LinuxSync,
        }
    }
}

impl SysEnv for LinuxSysEnv {
    fn time(&self) -> &dyn SysTime {
        &self.time
    }

    fn random(&self) -> &dyn SysRandom {
        &self.random
    }

    fn fs(&self) -> &dyn SysFs {
        &self.fs
    }

    fn net(&self) -> &dyn SysNet {
        &self.net
    }

    fn task_async(&self) -> &dyn SysTaskAsync {
        &self.task_async
    }

    fn task_sync(&self) -> &dyn SysTaskSync {
        &self.task_sync
    }

    fn sync(&self) -> &dyn SysSync {
        &self.sync
    }
}
