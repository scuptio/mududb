use crate::x_log::iou::{io_uring_event_loop, IOUSetting};
use crate::x_log::lsn_syncer::LSNSyncer;
use mudu::common::buf::Buf;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use tokio::sync::mpsc::Receiver;

struct XLogFileIOU {
    channel_name: String,
}

const SECTOR_SIZE: u64 = 512;

pub async fn f_sync_io_uring(
    file_path: Vec<String>,
    receiver: Receiver<(Buf, u64)>,
    lsn_syncer: LSNSyncer,
) -> RS<()> {
    io_uring_event_loop(
        file_path,
        receiver,
        |d| d,
        |u| lsn_syncer.ready(u),
        IOUSetting::default(),
    )
        .await
        .map_err(|e| m_error!(EC::IOErr, "io_uring event loop", e))?;

    Ok(())
}
