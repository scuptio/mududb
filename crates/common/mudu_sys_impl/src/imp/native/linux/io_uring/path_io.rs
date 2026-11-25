//! Path operation types and their pure submit/complete dispatch.
//!
//! Kept separate from the path futures so [`crate::imp::native::linux::io_uring::worker_ring`]
//! can dispatch path ops without depending back on the path module.

use std::ffi::CString;
use std::sync::Arc;
use tracing::{debug, trace};

use mudu::common::result::RS;

use crate::imp::io::user_io::{complete_op, completion_error, OpState};

/// A path operation queued for submission to the worker ring.
pub enum PathIoRequest {
    /// mkdirat for one path segment.
    CreateDir(PathCreateDirRequest),
    /// statx to test path existence.
    PathExists(PathExistsRequest),
    /// statx to read the file length.
    MetadataLen(PathMetadataLenRequest),
    /// unlinkat to remove a file.
    RemoveFile(PathRemoveFileRequest),
}

/// A path operation submitted to the ring and not yet completed; keeps
/// request-owned buffers alive until the CQE arrives.
pub enum PathInflightOp {
    /// In-flight mkdirat.
    CreateDir(Box<PathCreateDirRequest>),
    /// In-flight statx existence check.
    PathExists(Box<PathExistsRequest>),
    /// In-flight statx length query.
    MetadataLen(Box<PathMetadataLenRequest>),
    /// In-flight unlinkat.
    RemoveFile(Box<PathRemoveFileRequest>),
}

impl PathIoRequest {
    /// Returns a short static name of the operation kind for tracing.
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
    /// Returns a short static name of the operation kind for tracing.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CreateDir(_) => "path.mkdir",
            Self::PathExists(_) => "path.statx.exists",
            Self::MetadataLen(_) => "path.statx.len",
            Self::RemoveFile(_) => "path.unlink",
        }
    }
}

/// Request to create one directory via mkdirat.
pub struct PathCreateDirRequest {
    path: CString,
    mode: u32,
    state: Arc<OpState<()>>,
}

/// Request to test a path's existence via statx.
pub struct PathExistsRequest {
    path: CString,
    statx: Box<libc::statx>,
    state: Arc<OpState<bool>>,
}

/// Request to read a file's length via statx.
pub struct PathMetadataLenRequest {
    path: CString,
    statx: Box<libc::statx>,
    state: Arc<OpState<u64>>,
}

/// Request to remove a file via unlinkat.
pub struct PathRemoveFileRequest {
    path: CString,
    state: Arc<OpState<()>>,
}

impl PathCreateDirRequest {
    pub(crate) fn new(path: CString, mode: u32, state: Arc<OpState<()>>) -> Self {
        Self { path, mode, state }
    }

    pub(crate) fn path(&self) -> &CString {
        &self.path
    }

    pub(crate) fn mode(&self) -> u32 {
        self.mode
    }

    fn finish(self, result: RS<()>) {
        complete_op(self.state, result);
    }
}
impl PathExistsRequest {
    pub(crate) fn new(path: CString, state: Arc<OpState<bool>>) -> Self {
        Self {
            path,
            statx: Box::new(unsafe { std::mem::zeroed() }),
            state,
        }
    }

    pub(crate) fn path(&self) -> &CString {
        &self.path
    }

    fn finish(self, result: RS<bool>) {
        complete_op(self.state, result);
    }
}

impl PathMetadataLenRequest {
    pub(crate) fn new(path: CString, state: Arc<OpState<u64>>) -> Self {
        Self {
            path,
            statx: Box::new(unsafe { std::mem::zeroed() }),
            state,
        }
    }

    pub(crate) fn path(&self) -> &CString {
        &self.path
    }

    fn finish(self, result: RS<u64>) {
        complete_op(self.state, result);
    }
}

impl PathRemoveFileRequest {
    pub(crate) fn new(path: CString, state: Arc<OpState<()>>) -> Self {
        Self { path, state }
    }

    pub(crate) fn path(&self) -> &CString {
        &self.path
    }

    fn finish(self, result: RS<()>) {
        complete_op(self.state, result);
    }
}

/// Prepares the SQE for a queued path request and returns the inflight op
/// keeping request-owned buffers alive.
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
            trace!(path = %request.path().to_string_lossy(), "io_path submit statx exists");
            // The SQE stores the raw path pointer until the ring is submitted,
            // so it must reference the CString owned by the request. Passing a
            // temporary clone would leave a dangling pointer once the clone is
            // dropped at the end of this match arm.
            let path = &request.path;
            let statx = request.statx.as_mut() as *mut libc::statx;
            sqe.prep_statx(libc::AT_FDCWD, path, 0, libc::STATX_BASIC_STATS, statx);
            PathInflightOp::PathExists(Box::new(request))
        }
        PathIoRequest::MetadataLen(mut request) => {
            trace!(path = %request.path().to_string_lossy(), "io_path submit statx len");
            // See the PathExists arm above: the SQE must point at the
            // request-owned CString, not a dropped temporary clone.
            let path = &request.path;
            let statx = request.statx.as_mut() as *mut libc::statx;
            sqe.prep_statx(libc::AT_FDCWD, path, 0, libc::STATX_SIZE, statx);
            PathInflightOp::MetadataLen(Box::new(request))
        }
        PathIoRequest::RemoveFile(request) => {
            trace!(path = %request.path().to_string_lossy(), "io_path submit unlinkat");
            sqe.prep_unlinkat(libc::AT_FDCWD, request.path(), 0);
            PathInflightOp::RemoveFile(Box::new(request))
        }
    }
}

/// Completes an inflight path operation from its CQE result, delivering the
/// outcome to the waiting task. Returns `Ok(true)` when the op is done.
pub fn complete_path_io(_op_id: u64, op: PathInflightOp, result: i32) -> RS<bool> {
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
