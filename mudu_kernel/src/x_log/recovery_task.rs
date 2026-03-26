use crate::x_log::lsn_allocator::LSNAllocator;
use crate::x_log::lsn_syncer::LSNSyncer;
use crate::x_log::x_log_file::XLogFile;
use crate::x_log::xl_cfg::XLCfg;
use crate::x_log::xl_file_info::XLFileInfo;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_utils::notifier::NotifyWait;
use mudu_utils::sync::a_task::ATask;
use tracing::info;

type XLFileInfoSender = tokio::sync::oneshot::Sender<XLFileInfo>;
pub struct RecoveryTask {
    canceller: NotifyWait,
    task: String,
    recovery_done: NotifyWait,
    conf: XLCfg,
    vec_file_sender: Vec<XLFileInfoSender>,
    lsn_syncer: LSNSyncer,
    lsn_allocator: LSNAllocator,
}

impl RecoveryTask {
    pub fn new(
        canceller: NotifyWait,
        task: String,
        recovery_done: NotifyWait,
        conf: XLCfg,
        vec_file_sender: Vec<XLFileInfoSender>,
        lsn_syncer: LSNSyncer,
        lsn_allocator: LSNAllocator,
    ) -> Self {
        Self {
            canceller,
            task,
            recovery_done,
            conf,
            vec_file_sender,
            lsn_syncer,
            lsn_allocator,
        }
    }
}
#[async_trait]
impl ATask for RecoveryTask {
    fn notifier(&self) -> NotifyWait {
        self.canceller.clone()
    }

    fn name(&self) -> String {
        self.task.clone()
    }

    async fn run(self) -> RS<()> {
        let cfg = self.conf;
        let mut vec_file_sender = self.vec_file_sender;

        let mut vec_log_file = vec![];
        for _n in 0..cfg.x_log_channels {
            let name = (_n + 1).to_string();
            let x_log_file = XLogFile::recovery(cfg.clone(), name.clone()).await?;
            vec_log_file.push(x_log_file.file_info());
        }

        // todo! compute last lsn
        let last_lsn = 0;
        self.lsn_syncer.recovery(last_lsn)?;
        self.lsn_allocator.recovery(last_lsn)?;

        for f in vec_log_file.into_iter().rev() {
            let sender = vec_file_sender.pop().unwrap();
            let r = sender.send(f);
            match r {
                Ok(()) => {}
                Err(_) => {
                    panic!("oneshot channel send error")
                }
            }
        }
        let _ = self.recovery_done.notify_all();
        info!("recovery task finished");
        Ok(())
    }
}
