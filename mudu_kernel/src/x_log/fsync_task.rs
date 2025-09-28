use crate::contract::a_task::ATask;
use crate::contract::lsn::LSN;
use crate::x_log::lsn_syncer::LSNSyncer;
use crate::x_log::x_log_file::XLogFile;
#[cfg(target_os = "linux")]
use crate::x_log::x_log_file_iou::f_sync_io_uring;
use crate::x_log::xl_file_info::XLFileInfo;
#[cfg(target_os = "linux")]
use crate::x_log::xl_path::xl_file_path;
use mudu::common::buf::Buf;
use mudu::common::result::RS;
use mudu_utils::notifier::Notifier;
use mudu_utils::task_trace;
use tokio::sync::mpsc::Receiver;
use tokio::sync::oneshot;
use tracing::error;

#[cfg(target_os = "linux")]
async fn sync_io(
    f: XLFileInfo,
    receiver: Receiver<(Buf, LSN)>,
    lsn_syncer: LSNSyncer,
) -> RS<()> {
    if f.cfg.x_log_use_io_uring {
        let path_buf = xl_file_path(
            &f.cfg.x_log_path,
            &f.channel_name,
            &f.cfg.x_log_ext_name,
            f.file_no,
        );
        let path_string = path_buf
            .as_path()
            .to_str()
            .expect("expected path to string")
            .to_string();
        f_sync_io_uring(vec![path_string], receiver, lsn_syncer).await?;
    } else {
        let mut file = XLogFile::new(f.cfg, f.channel_name, f.file_size, f.file_no)?;
        file.f_sync_loop(receiver, lsn_syncer).await?;
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
async fn sync_io(
    f: XLFileInfo,
    receiver: Receiver<(Buf, LSN)>,
    lsn_syncer: LSNSyncer,
) -> RS<()> {
    let mut file = XLogFile::new(f.cfg, f.channel_name, f.file_size, f.file_no)?;
    file.f_sync_loop(receiver, lsn_syncer).await?;
    Ok(())
}

impl FsyncTask {
    pub fn new(
        canceller: Notifier,
        name: String,
        x_log_file: XLogFileReceiver,
        receiver: Receiver<(Buf, LSN)>,
        lsn_syncer: LSNSyncer,
    ) -> Self {
        Self {
            canceller,
            name,
            file_receiver: x_log_file,
            log_receiver: receiver,
            lsn_syncer,
        }
    }
}

impl ATask for FsyncTask {
    fn notifier(&self) -> Notifier {
        self.canceller.clone()
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(self) -> RS<()> {
        self.run_fsync_task().await?;
        Ok(())
    }
}

impl FsyncTask {
    async fn run_fsync_task(self) -> RS<()> {
        let _trace = task_trace!();
        let r = Self::fsync_task(self.file_receiver, self.log_receiver, self.lsn_syncer).await;
        match r {
            Ok(_) => {}
            Err(e) => {
                error!("fsync task error: {}", e);
                return Err(e);
            }
        }
        Ok(())
    }

    async fn fsync_task(
        x_log_file_ch: XLogFileReceiver,
        receiver: Receiver<(Buf, LSN)>,
        lsn_syncer: LSNSyncer,
    ) -> RS<()> {
        let _trace = task_trace!();
        let r = x_log_file_ch.await;
        let f = match r {
            Ok(f) => f,
            Err(e) => {
                panic!("receive error {:?}", e)
            }
        };
        sync_io(f, receiver, lsn_syncer).await?;
        Ok(())
    }
}

pub struct FsyncTask {
    canceller: Notifier,
    name: String,
    file_receiver: XLogFileReceiver,
    log_receiver: Receiver<(Buf, LSN)>,
    lsn_syncer: LSNSyncer,
}

pub type XLogFileReceiver = oneshot::Receiver<XLFileInfo>;
