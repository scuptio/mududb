use std::ffi::CString;
use std::future::Future;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tracing::{debug, trace};

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

use crate::imp::io::user_io::{op_state, poll_op, OpState};
use crate::imp::native::linux::io_uring::path_io::{
    PathCreateDirRequest, PathExistsRequest, PathIoRequest, PathMetadataLenRequest,
    PathRemoveFileRequest,
};
use crate::imp::native::linux::io_uring::worker_ring::{
    has_current_worker_ring, with_current_ring, WorkerRingOp,
};

#[cfg(feature = "debug_trace")]
type PendingTaskTrace = crate::task::trace::TaskTrace;
#[cfg(not(feature = "debug_trace"))]
type PendingTaskTrace = crate::task::trace::NoopTaskTrace;

enum PathFutureState<T> {
    Init,
    Pending {
        state: Arc<OpState<T>>,
        _trace: PendingTaskTrace,
    },
    Done,
}

pub(crate) struct PathCreateDirFuture {
    path: Option<CString>,
    mode: u32,
    state: PathFutureState<()>,
}

pub(crate) struct PathExistsFuture {
    path: Option<CString>,
    state: PathFutureState<bool>,
}

pub(crate) struct PathMetadataLenFuture {
    path: Option<CString>,
    state: PathFutureState<u64>,
}

pub(crate) struct PathRemoveFileFuture {
    path: Option<CString>,
    state: PathFutureState<()>,
}

impl PathCreateDirFuture {
    pub(crate) fn new(path: CString, mode: u32) -> Self {
        Self {
            path: Some(path),
            mode,
            state: PathFutureState::Init,
        }
    }
}

impl PathExistsFuture {
    pub(crate) fn new(path: CString) -> Self {
        Self {
            path: Some(path),
            state: PathFutureState::Init,
        }
    }
}

impl PathMetadataLenFuture {
    pub(crate) fn new(path: CString) -> Self {
        Self {
            path: Some(path),
            state: PathFutureState::Init,
        }
    }
}

impl PathRemoveFileFuture {
    pub(crate) fn new(path: CString) -> Self {
        Self {
            path: Some(path),
            state: PathFutureState::Init,
        }
    }
}

impl Future for PathCreateDirFuture {
    type Output = RS<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            PathFutureState::Init => {
                let state = op_state();
                let Some(path) = self.path.take() else {
                    self.state = PathFutureState::Done;
                    return Poll::Ready(Err(mudu_error!(
                        ErrorCode::Internal,
                        "PathCreateDirFuture polled after completion"
                    )));
                };
                if let Err(err) = with_current_ring(|ring| {
                    debug!("create dir {}", path.to_string_lossy());
                    ring.register(WorkerRingOp::Path(PathIoRequest::CreateDir(
                        PathCreateDirRequest::new(path, self.mode, state.clone()),
                    )))
                    .map(|_| ())
                }) {
                    self.state = PathFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = PathFutureState::Pending {
                    state,
                    _trace: crate::task_trace!(),
                };
                self.poll(cx)
            }
            PathFutureState::Pending { state, .. } => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = PathFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            PathFutureState::Done => Poll::Pending,
        }
    }
}

impl Future for PathExistsFuture {
    type Output = RS<bool>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            PathFutureState::Init => {
                let state = op_state();
                let Some(path) = self.path.take() else {
                    self.state = PathFutureState::Done;
                    return Poll::Ready(Err(mudu_error!(
                        ErrorCode::Internal,
                        "PathExistsFuture polled after completion"
                    )));
                };
                if let Err(err) = with_current_ring(|ring| {
                    ring.register(WorkerRingOp::Path(PathIoRequest::PathExists(
                        PathExistsRequest::new(path, state.clone()),
                    )))
                    .map(|_| ())
                }) {
                    self.state = PathFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = PathFutureState::Pending {
                    state,
                    _trace: crate::task_trace!(),
                };
                self.poll(cx)
            }
            PathFutureState::Pending { state, .. } => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = PathFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            PathFutureState::Done => Poll::Pending,
        }
    }
}

impl Future for PathMetadataLenFuture {
    type Output = RS<u64>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            PathFutureState::Init => {
                let state = op_state();
                let Some(path) = self.path.take() else {
                    self.state = PathFutureState::Done;
                    return Poll::Ready(Err(mudu_error!(
                        ErrorCode::Internal,
                        "PathMetadataLenFuture polled after completion"
                    )));
                };
                if let Err(err) = with_current_ring(|ring| {
                    ring.register(WorkerRingOp::Path(PathIoRequest::MetadataLen(
                        PathMetadataLenRequest::new(path, state.clone()),
                    )))
                    .map(|_| ())
                }) {
                    self.state = PathFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = PathFutureState::Pending {
                    state,
                    _trace: crate::task_trace!(),
                };
                self.poll(cx)
            }
            PathFutureState::Pending { state, .. } => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = PathFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            PathFutureState::Done => Poll::Pending,
        }
    }
}

impl Future for PathRemoveFileFuture {
    type Output = RS<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            PathFutureState::Init => {
                let state = op_state();
                let Some(path) = self.path.take() else {
                    self.state = PathFutureState::Done;
                    return Poll::Ready(Err(mudu_error!(
                        ErrorCode::Internal,
                        "PathRemoveFileFuture polled after completion"
                    )));
                };
                if let Err(err) = with_current_ring(|ring| {
                    ring.register(WorkerRingOp::Path(PathIoRequest::RemoveFile(
                        PathRemoveFileRequest::new(path, state.clone()),
                    )))
                    .map(|_| ())
                }) {
                    self.state = PathFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = PathFutureState::Pending {
                    state,
                    _trace: crate::task_trace!(),
                };
                self.poll(cx)
            }
            PathFutureState::Pending { state, .. } => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = PathFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            PathFutureState::Done => Poll::Pending,
        }
    }
}

/// Creates `path` and all missing parent directories. Uses the current
/// worker ring when one is installed (mkdirat per segment), otherwise falls
/// back to the default IO provider.
pub async fn create_dir_all(path: &Path) -> RS<()> {
    if !has_current_worker_ring() {
        return crate::fs::async_::create_dir_all(path).await;
    }
    for segment in path_prefixes(path) {
        trace!(segment = %segment.display(), "io_path create_dir_all segment");
        let c_path = path_to_cstring(&segment)?;
        PathCreateDirFuture::new(c_path, 0o755).await?;
    }
    Ok(())
}

/// Returns `true` if a filesystem object exists at `path` (statx on the
/// worker ring, or a synchronous metadata probe off-ring).
pub async fn path_exists(path: &Path) -> RS<bool> {
    if !has_current_worker_ring() {
        return Ok(std::fs::metadata(path).is_ok());
    }
    PathExistsFuture::new(path_to_cstring(path)?).await
}

/// Returns the length in bytes of the file at `path` (statx on the worker
/// ring, or the default IO provider off-ring).
pub async fn metadata_len(path: &Path) -> RS<u64> {
    if !has_current_worker_ring() {
        return crate::fs::async_::metadata_len(path).await;
    }
    PathMetadataLenFuture::new(path_to_cstring(path)?).await
}

/// Removes the file at `path`; succeeds silently if it does not exist
/// (unlinkat on the worker ring, or the default IO provider off-ring).
pub async fn remove_file_if_exists(path: &Path) -> RS<()> {
    if !has_current_worker_ring() {
        return crate::fs::async_::remove_file_if_exists(path).await;
    }
    PathRemoveFileFuture::new(path_to_cstring(path)?).await
}

pub(crate) fn path_to_cstring(path: impl AsRef<Path>) -> RS<CString> {
    CString::new(path.as_ref().as_os_str().as_bytes()).map_err(|_| {
        mudu_error!(
            ErrorCode::Encode,
            format!("path contains interior NUL: {}", path.as_ref().display())
        )
    })
}

pub(crate) fn path_prefixes(path: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut prefixes = Vec::new();
    let mut current = PathBuf::new();
    for component in path.as_ref().components() {
        match component {
            Component::Prefix(prefix) => current.push(prefix.as_os_str()),
            Component::RootDir => current.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => current.push(component.as_os_str()),
            Component::Normal(part) => {
                current.push(part);
                prefixes.push(current.clone());
            }
        }
    }
    prefixes
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::imp::io::user_io::try_take_op;
    use crate::imp::native::linux::io_uring::path_io::{complete_path_io, PathInflightOp};

    fn test_cstring() -> CString {
        CString::new("/tmp/mudu-iou-path-test").unwrap()
    }

    #[test]
    fn path_request_kinds_and_accessors() {
        let create = PathCreateDirRequest::new(test_cstring(), 0o755, op_state());
        assert_eq!(create.path().as_bytes(), b"/tmp/mudu-iou-path-test");
        assert_eq!(create.mode(), 0o755);
        let exists = PathExistsRequest::new(test_cstring(), op_state());
        assert_eq!(exists.path().as_bytes(), b"/tmp/mudu-iou-path-test");
        let len = PathMetadataLenRequest::new(test_cstring(), op_state());
        assert_eq!(len.path().as_bytes(), b"/tmp/mudu-iou-path-test");
        let remove = PathRemoveFileRequest::new(test_cstring(), op_state());
        assert_eq!(remove.path().as_bytes(), b"/tmp/mudu-iou-path-test");

        assert_eq!(PathIoRequest::CreateDir(create).kind(), "path.mkdir");
        assert_eq!(
            PathIoRequest::PathExists(exists).kind(),
            "path.statx.exists"
        );
        assert_eq!(PathIoRequest::MetadataLen(len).kind(), "path.statx.len");
        assert_eq!(PathIoRequest::RemoveFile(remove).kind(), "path.unlink");

        assert_eq!(
            PathInflightOp::CreateDir(Box::new(PathCreateDirRequest::new(
                test_cstring(),
                0o755,
                op_state()
            )))
            .kind(),
            "path.mkdir"
        );
        assert_eq!(
            PathInflightOp::PathExists(Box::new(PathExistsRequest::new(
                test_cstring(),
                op_state()
            )))
            .kind(),
            "path.statx.exists"
        );
        assert_eq!(
            PathInflightOp::MetadataLen(Box::new(PathMetadataLenRequest::new(
                test_cstring(),
                op_state()
            )))
            .kind(),
            "path.statx.len"
        );
        assert_eq!(
            PathInflightOp::RemoveFile(Box::new(PathRemoveFileRequest::new(
                test_cstring(),
                op_state()
            )))
            .kind(),
            "path.unlink"
        );
    }

    #[test]
    fn complete_path_io_error_and_edge_results() {
        // mkdirat: a generic negative result is an error.
        let state = op_state();
        let op = PathInflightOp::CreateDir(Box::new(PathCreateDirRequest::new(
            test_cstring(),
            0o755,
            state.clone(),
        )));
        assert!(complete_path_io(1, op, -libc::EPERM).unwrap());
        assert!(try_take_op(&state).unwrap().is_err());

        // statx exists: ENOENT maps to `false`, other negatives to an error.
        let state = op_state();
        let op = PathInflightOp::PathExists(Box::new(PathExistsRequest::new(
            test_cstring(),
            state.clone(),
        )));
        assert!(complete_path_io(2, op, -libc::ENOENT).unwrap());
        assert!(!try_take_op(&state).unwrap().unwrap());

        let state = op_state();
        let op = PathInflightOp::PathExists(Box::new(PathExistsRequest::new(
            test_cstring(),
            state.clone(),
        )));
        assert!(complete_path_io(3, op, -libc::EACCES).unwrap());
        assert!(try_take_op(&state).unwrap().is_err());

        // statx len: negative results error out.
        let state = op_state();
        let op = PathInflightOp::MetadataLen(Box::new(PathMetadataLenRequest::new(
            test_cstring(),
            state.clone(),
        )));
        assert!(complete_path_io(4, op, -libc::ENOENT).unwrap());
        assert!(try_take_op(&state).unwrap().is_err());

        // unlinkat: ENOENT is success (idempotent removal), others error.
        let state = op_state();
        let op = PathInflightOp::RemoveFile(Box::new(PathRemoveFileRequest::new(
            test_cstring(),
            state.clone(),
        )));
        assert!(complete_path_io(5, op, -libc::ENOENT).unwrap());
        assert!(try_take_op(&state).unwrap().is_ok());

        let state = op_state();
        let op = PathInflightOp::RemoveFile(Box::new(PathRemoveFileRequest::new(
            test_cstring(),
            state.clone(),
        )));
        assert!(complete_path_io(6, op, -libc::EACCES).unwrap());
        assert!(try_take_op(&state).unwrap().is_err());
    }

    #[test]
    fn path_to_cstring_rejects_nul_and_prefixes_expand() {
        assert!(path_to_cstring("/tmp/a\0b").is_err());
        let prefixes = path_prefixes("/a/b/c");
        assert_eq!(prefixes.len(), 3);
        assert_eq!(prefixes[0], PathBuf::from("/a"));
        assert_eq!(prefixes[2], PathBuf::from("/a/b/c"));
    }
}
