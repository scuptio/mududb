use crate::io::path;

use mudu::common::result::RS;

use std::path::Path;

use crate::io::path::path_to_cstring;
use crate::io::worker_ring::has_current_worker_ring;
use crate::scoped_task_trace;
use tracing::trace;

pub(crate) async fn create_dir_all(path: impl AsRef<Path>) -> RS<()> {
    scoped_task_trace!();
    iou_create_dir_all(path).await
}

pub(crate) async fn metadata_len(path: impl AsRef<Path>) -> RS<u64> {
    iou_metadata_len(path).await
}

pub(crate) async fn path_exists(path: impl AsRef<Path>) -> RS<bool> {
    iou_path_exists(path).await
}

pub(crate) async fn remove_file_if_exists(path: impl AsRef<Path>) -> RS<()> {
    iou_remove_file_if_exists(path).await
}

pub async fn iou_create_dir_all(path: impl AsRef<Path>) -> RS<()> {
    scoped_task_trace!();
    if has_current_worker_ring() {
        for segment in path::path_prefixes(path) {
            trace!(segment = %segment.display(), "io_path create_dir_all segment");
            let c_path = path_to_cstring(&segment)?;
            path::PathCreateDirFuture::new(c_path, 0o755).await?;
        }
    } else {
        panic!("do not support io_uring operation iou_create_dir_all")
    }

    Ok(())
}

pub async fn iou_metadata_len(path: impl AsRef<Path>) -> RS<u64> {
    if has_current_worker_ring() {
        path::PathMetadataLenFuture::new(path_to_cstring(path)?).await
    } else {
        panic!("do not support io_uring operation iou_metadata_len")
    }
}

pub async fn iou_path_exists(path: impl AsRef<Path>) -> RS<bool> {
    if has_current_worker_ring() {
        path::PathExistsFuture::new(path_to_cstring(path)?).await
    } else {
        panic!("do not support io_uring operation iou_path_exists")
    }
}

pub async fn iou_remove_file_if_exists(path: impl AsRef<Path>) -> RS<()> {
    if has_current_worker_ring() {
        path::PathRemoveFileFuture::new(path_to_cstring(path)?).await
    } else {
        panic!("do not support io_uring operation iou_remove_file_if_exists")
    }
}
