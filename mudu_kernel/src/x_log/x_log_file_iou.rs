use crate::common::buf::Buf;
use crate::common::error::ER;
use crate::common::result::RS;
use crate::x_log::iou::{io_uring_event_loop, IOUSetting};
use crate::x_log::lsn_syncer::LSNSyncer;
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
        .map_err(|e| ER::IOError(e.to_string()))?;

    Ok(())
}
