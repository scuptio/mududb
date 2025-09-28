use crate::contract::a_task::{ATaskRef, AsyncTask};
use crate::contract::x_log::XLog;
use crate::sync::s_mutex::SMutex;
use crate::sync::unique_inner::UniqueInner;
use crate::x_log::fsync_task::FsyncTask;
use crate::x_log::lsn_allocator::LSNAllocator;
use crate::x_log::lsn_syncer::LSNSyncer;
use crate::x_log::recovery_task::RecoveryTask;
use crate::x_log::x_log_impl::XLogImpl;
use crate::x_log::xl_cfg::XLCfg;
use crate::x_log::xl_file_info::XLFileInfo;
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu_utils::notifier::Notifier;
use std::fs;
use std::sync::Arc;
use tokio::sync::mpsc::channel;
use tokio::sync::oneshot;

pub struct XLogService {
    lsn_syncer: LSNSyncer,
    lsn_allocator: LSNAllocator,
    recovery_task: SMutex<Option<AsyncTask>>,
    x_log_channel: SMutex<Vec<(Arc<dyn XLog>, AsyncTask)>>,
}

/// 10MB
const LOG_FILE_LIMIT: usize = 1024 * 1024 * 10;

impl XLogService {
    pub fn new(cfg: XLCfg, canceller: Notifier, recovery_done: Notifier) -> RS<Self> {
        let lsn_syncer = LSNSyncer::new(0);
        let lsn_allocator = LSNAllocator::new();

        let mut vec_x_log: Vec<(Arc<dyn XLog>, AsyncTask)> = vec![];
        let mut vec_file_sender = vec![];
        if !fs::exists(&cfg.x_log_path)
            .map_err(|e| m_error!(ER::IOErr, "XLogService::new, fs::exists error", e))?
        {
            fs::create_dir_all(&cfg.x_log_path).map_err(|e| {
                m_error!(ER::IOErr, "XLogService::new, fs::create_dir_all error", e)
            })?
        }
        for _n in 0..cfg.x_log_channels {
            let (sender, receiver) = channel(10000);
            let name = (_n + 1).to_string();
            let (x_log_file_s, x_log_file_r) = oneshot::channel::<XLFileInfo>();
            let param = FsyncTask::new(
                canceller.clone(),
                name,
                x_log_file_r,
                receiver,
                lsn_syncer.clone(),
            );
            vec_file_sender.push(x_log_file_s);
            let f_task: Arc<dyn ATaskRef> = Arc::new(UniqueInner::new(param));
            let x_log: Arc<dyn XLog> = Arc::new(XLogImpl::new(
                lsn_allocator.clone(),
                lsn_syncer.clone(),
                vec![sender],
            ));
            vec_x_log.push((x_log, f_task));
        }

        let recovery_task: Option<AsyncTask> = Some(Arc::new(UniqueInner::new(RecoveryTask::new(
            canceller.clone(),
            "recovery".to_string(),
            recovery_done,
            cfg,
            vec_file_sender,
            lsn_syncer.clone(),
            lsn_allocator.clone(),
        ))));

        Ok(Self {
            lsn_syncer,
            lsn_allocator,
            recovery_task: SMutex::new(recovery_task),
            x_log_channel: SMutex::new(vec_x_log),
        })
    }

    /// transaction log channel, the x log and its fsync task should be run in the same thread
    pub fn x_log_channel(&self) -> Vec<(Arc<dyn XLog>, AsyncTask)> {
        let mut x_log = vec![];
        let mut _x_log = self.x_log_channel.lock().unwrap();
        std::mem::swap(&mut *_x_log, &mut x_log);
        x_log
    }

    pub fn recovery_task(&self) -> AsyncTask {
        let r = self.recovery_task.lock();
        let mut guard = match r {
            Ok(g) => g,
            Err(e) => {
                panic!("recovery_task mutex poisoned: {}", e);
            }
        };

        let mut task: Option<AsyncTask> = None;
        std::mem::swap(&mut *guard, &mut task);
        match task {
            Some(task) => task,
            None => {
                panic!("recovery_task can only be invoked once");
            }
        }
    }
}

impl Drop for XLogService {
    fn drop(&mut self) {
        self.lsn_syncer.finalize();
    }
}
