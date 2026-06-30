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

use crate::imp::io::user_io::{complete_op, completion_error, op_state, poll_op, OpState};
use crate::imp::native::linux::io_uring::worker_ring::{
    has_current_worker_ring, with_current_ring, WorkerLocalRing, WorkerRingOp,
};

#[cfg(feature = "debug_trace")]
type PendingTaskTrace = crate::task::trace::TaskTrace;
#[cfg(not(feature = "debug_trace"))]
type PendingTaskTrace = crate::task::trace::NoopTaskTrace;

pub enum PathIoRequest {
    CreateDir(PathCreateDirRequest),
    PathExists(PathExistsRequest),
    MetadataLen(PathMetadataLenRequest),
    RemoveFile(PathRemoveFileRequest),
}

pub enum PathInflightOp {
    CreateDir(Box<PathCreateDirRequest>),
    PathExists(Box<PathExistsRequest>),
    MetadataLen(Box<PathMetadataLenRequest>),
    RemoveFile(Box<PathRemoveFileRequest>),
}

impl PathIoRequest {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CreateDir(_) => "path.mkdir",
            Self::PathExists(_) => "path.statx.exists",
            Self::MetadataLen(_) => "path.statx.len",
            Self::RemoveFile(_) => "path.unlink",
        }
    }
}

impl PathInflightOp {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CreateDir(_) => "path.mkdir",
            Self::PathExists(_) => "path.statx.exists",
            Self::MetadataLen(_) => "path.statx.len",
            Self::RemoveFile(_) => "path.unlink",
        }
    }
}

pub struct PathCreateDirRequest {
    path: CString,
    mode: u32,
    state: Arc<OpState<()>>,
}

pub struct PathExistsRequest {
    path: CString,
    statx: Box<libc::statx>,
    state: Arc<OpState<bool>>,
}

pub struct PathMetadataLenRequest {
    path: CString,
    statx: Box<libc::statx>,
    state: Arc<OpState<u64>>,
}

pub struct PathRemoveFileRequest {
    path: CString,
    state: Arc<OpState<()>>,
}

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

impl PathCreateDirRequest {
    fn new(path: CString, mode: u32, state: Arc<OpState<()>>) -> Self {
        Self { path, mode, state }
    }

    fn path(&self) -> &CString {
        &self.path
    }

    fn mode(&self) -> u32 {
        self.mode
    }

    fn finish(self, result: RS<()>) {
        complete_op(self.state, result);
    }
}

impl PathExistsRequest {
    fn new(path: CString, state: Arc<OpState<bool>>) -> Self {
        Self {
            path,
            statx: Box::new(unsafe { std::mem::zeroed() }),
            state,
        }
    }

    fn path(&self) -> &CString {
        &self.path
    }

    fn statx_mut_ptr(&mut self) -> *mut libc::statx {
        self.statx.as_mut()
    }

    fn finish(self, result: RS<bool>) {
        complete_op(self.state, result);
    }
}

impl PathMetadataLenRequest {
    fn new(path: CString, state: Arc<OpState<u64>>) -> Self {
        Self {
            path,
            statx: Box::new(unsafe { std::mem::zeroed() }),
            state,
        }
    }

    fn path(&self) -> &CString {
        &self.path
    }

    fn statx_mut_ptr(&mut self) -> *mut libc::statx {
        self.statx.as_mut()
    }

    fn finish(self, result: RS<u64>) {
        complete_op(self.state, result);
    }
}

impl PathRemoveFileRequest {
    fn new(path: CString, state: Arc<OpState<()>>) -> Self {
        Self { path, state }
    }

    fn path(&self) -> &CString {
        &self.path
    }

    fn finish(self, result: RS<()>) {
        complete_op(self.state, result);
    }
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

pub async fn path_exists(path: &Path) -> RS<bool> {
    if !has_current_worker_ring() {
        return Ok(std::fs::metadata(path).is_ok());
    }
    PathExistsFuture::new(path_to_cstring(path)?).await
}

pub async fn metadata_len(path: &Path) -> RS<u64> {
    if !has_current_worker_ring() {
        return crate::fs::async_::metadata_len(path).await;
    }
    PathMetadataLenFuture::new(path_to_cstring(path)?).await
}

pub async fn remove_file_if_exists(path: &Path) -> RS<()> {
    if !has_current_worker_ring() {
        return crate::fs::async_::remove_file_if_exists(path).await;
    }
    PathRemoveFileFuture::new(path_to_cstring(path)?).await
}

pub fn submit_path_io(
    request: PathIoRequest,
    sqe: &mut crate::imp::native::linux::io_uring::iouring::SubmissionQueueEntry<'_>,
) -> PathInflightOp {
    match request {
        PathIoRequest::CreateDir(request) => {
            debug!(path = %request.path().to_string_lossy(), "io_path submit mkdirat");
            sqe.prep_mkdirat(libc::AT_FDCWD, request.path(), request.mode());
            PathInflightOp::CreateDir(Box::new(request))
        }
        PathIoRequest::PathExists(mut request) => {
            let path = request.path().clone();
            trace!(path = %path.to_string_lossy(), "io_path submit statx exists");
            sqe.prep_statx(
                libc::AT_FDCWD,
                &path,
                0,
                libc::STATX_BASIC_STATS,
                request.statx_mut_ptr(),
            );
            PathInflightOp::PathExists(Box::new(request))
        }
        PathIoRequest::MetadataLen(mut request) => {
            let path = request.path().clone();
            trace!(path = %path.to_string_lossy(), "io_path submit statx len");
            sqe.prep_statx(
                libc::AT_FDCWD,
                &path,
                0,
                libc::STATX_SIZE,
                request.statx_mut_ptr(),
            );
            PathInflightOp::MetadataLen(Box::new(request))
        }
        PathIoRequest::RemoveFile(request) => {
            trace!(path = %request.path().to_string_lossy(), "io_path submit unlinkat");
            sqe.prep_unlinkat(libc::AT_FDCWD, request.path(), 0);
            PathInflightOp::RemoveFile(Box::new(request))
        }
    }
}

pub fn complete_path_io(
    _op_id: u64,
    op: PathInflightOp,
    result: i32,
    _ring: &WorkerLocalRing,
) -> RS<bool> {
    match op {
        PathInflightOp::CreateDir(request) => {
            debug!(path = %request.path().to_string_lossy(), result, "io_path complete mkdirat");
            if result < 0 && result != -libc::EEXIST {
                request.finish(Err(completion_error("mkdirat", result)));
            } else {
                request.finish(Ok(()));
            }
            Ok(true)
        }
        PathInflightOp::PathExists(request) => {
            trace!(path = %request.path().to_string_lossy(), result, "io_path complete statx exists");
            if result == -libc::ENOENT {
                request.finish(Ok(false));
            } else if result < 0 {
                request.finish(Err(completion_error("statx", result)));
            } else {
                request.finish(Ok(true));
            }
            Ok(true)
        }
        PathInflightOp::MetadataLen(request) => {
            trace!(path = %request.path().to_string_lossy(), result, "io_path complete statx len");
            if result < 0 {
                request.finish(Err(completion_error("metalen statx", result)));
            } else {
                let len = request.statx.stx_size;
                request.finish(Ok(len));
            }
            Ok(true)
        }
        PathInflightOp::RemoveFile(request) => {
            trace!(path = %request.path().to_string_lossy(), result, "io_path complete unlinkat");
            if result == -libc::ENOENT {
                request.finish(Ok(()));
            } else if result < 0 {
                request.finish(Err(completion_error("unlinkat", result)));
            } else {
                request.finish(Ok(()));
            }
            Ok(true)
        }
    }
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
