use std::any::Any;
#[cfg(target_os = "linux")]
use std::ffi::{CStr, CString};
#[cfg(target_os = "linux")]
use std::future::Future;
use std::future::poll_fn;
use std::mem::ManuallyDrop;
use std::path::Path;
#[cfg(target_os = "linux")]
use std::pin::Pin;
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::task::{Context, Poll};

#[cfg(target_os = "linux")]
use crate::io::user_io::completion_error;
use crate::io::user_io::{OpState, poll_op, try_take_op};
#[cfg(target_os = "linux")]
use crate::io::user_io::{complete_op, op_state};
#[cfg(target_os = "linux")]
use crate::io::worker_ring::{WorkerLocalRing, WorkerRingOp, with_current_ring};
use crate::io::sys_file::SysFile;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use crate::sync::a_mutex::AMutex;
use crate::{scoped_task_trace, task_trace};
#[cfg(unix)]
use std::os::fd::{FromRawFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{FromRawHandle, IntoRawHandle, RawHandle};

#[cfg(windows)]
type RawFd = usize;

pub type File = crate::tokio::fs::File;
pub type TFile = crate::tokio::fs::File;

#[derive(Debug)]
pub struct IoFile {
    fd: RawFd,
    portable_io_lock: Arc<AMutex<()>>,
}

#[derive(Clone)]
pub struct WriteHandle {
    pub state: Arc<OpState<usize>>,
}

#[derive(Clone)]
pub struct FlushHandle<P> {
    pub state: Arc<OpState<Box<dyn Any + Send>>>,
    pub _marker: std::marker::PhantomData<P>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OptionWrite {
    pub blind_write: bool,
}

#[cfg(target_os = "linux")]
pub enum FileIoRequest {
    Open(FileOpenRequest),
    Close(FileCloseRequest),
    Read(FileReadRequest),
    Write(FileWriteRequest),
    Flush(FileFlushRequest),
    Len(FileLenRequest),
}

#[cfg(target_os = "linux")]
pub enum FileInflightOp {
    Open(Box<FileOpenRequest>),
    Close(Box<FileCloseRequest>),
    Read {
        request: Box<FileReadRequest>,
        buf: Vec<u8>,
    },
    Write(Box<FileWriteRequest>),
    Flush(Box<FileFlushRequest>),
    Len(Box<FileLenRequest>),
}

#[cfg(target_os = "linux")]
pub struct FileOpenRequest {
    path: CString,
    flags: i32,
    mode: u32,
    state: Arc<OpState<RawFd>>,
}

#[cfg(target_os = "linux")]
pub struct FileCloseRequest {
    fd: RawFd,
    state: Arc<OpState<()>>,
}

#[cfg(target_os = "linux")]
pub struct FileReadRequest {
    fd: RawFd,
    len: usize,
    offset: u64,
    state: Arc<OpState<Vec<u8>>>,
}

#[cfg(target_os = "linux")]
pub struct FileWriteRequest {
    fd: RawFd,
    offset: u64,
    data: Vec<u8>,
    written: usize,
    blind_write: bool,
    state: Arc<OpState<usize>>,
}

#[cfg(target_os = "linux")]
pub struct FileFlushRequest {
    fd: RawFd,
    payload: Option<Box<dyn Any + Send>>,
    state: Arc<OpState<Box<dyn Any + Send>>>,
}

#[cfg(target_os = "linux")]
pub struct FileLenRequest {
    fd: RawFd,
    statx: Box<libc::statx>,
    state: Arc<OpState<u64>>,
}

impl Default for IoFile {
    fn default() -> Self {
        Self {
            #[cfg(unix)]
            fd: 0,
            #[cfg(windows)]
            fd: 0,
            portable_io_lock: Arc::new(AMutex::new(())),
        }
    }
}

impl IoFile {
    pub fn is_invalid(&self) -> bool {
        #[cfg(unix)]
        {
            self.fd == 0
        }
        #[cfg(windows)]
        {
            self.fd == 0
        }
    }

    pub fn new(fd: RawFd) -> Self {
        Self {
            fd,
            portable_io_lock: Arc::new(AMutex::new(())),
        }
    }
}
pub async fn open<P: AsRef<Path>>(path: P, flags: i32, mode: u32) -> RS<IoFile> {
    scoped_task_trace!();
    #[cfg(target_os = "linux")]
    if crate::io::worker_ring::has_current_worker_ring() {
        let path = CString::new(path.as_ref().as_os_str().as_encoded_bytes())
            .map_err(|_| m_error!(EC::ParseErr, "path contains NUL byte"))?;
        let fd = FileOpenFuture::new(path, flags, mode).await?;
        return Ok(IoFile::from_raw_fd(fd));
    }

    open_async_portable(path.as_ref(), flags, mode).await
}

pub async fn close(file: IoFile) -> RS<()> {
    #[cfg(target_os = "linux")]
    if crate::io::worker_ring::has_current_worker_ring() {
        return FileCloseFuture::new(file.fd).await;
    }

    close_async_portable(file).await
}

pub async fn read(file: &IoFile, len: usize, offset: u64) -> RS<Vec<u8>> {
    #[cfg(target_os = "linux")]
    if crate::io::worker_ring::has_current_worker_ring() {
        return FileReadFuture::new(file.fd, len, offset).await;
    }

    read_async_portable(file, len, offset).await
}

pub async fn file_len(file: &IoFile) -> RS<u64> {
    #[cfg(target_os = "linux")]
    if crate::io::worker_ring::has_current_worker_ring() {
        return FileLenFuture::new(file.fd).await;
    }

    metadata_len_by_file(file)
}

pub async fn metadata_len<P: AsRef<Path>>(path: P) -> RS<u64> {
    crate::io::fs::metadata_len(path.as_ref()).await
        .map_err(|e| m_error!(EC::IOErr, "read file metadata error", e))
}

pub fn metadata_len_by_file(file: &IoFile) -> RS<u64> {
    with_std_file(file, |std_file| {
        std_file
            .metadata()
            .map(|metadata| metadata.len())
            .map_err(|e| m_error!(EC::IOErr, "read file metadata by fd error", e))
    })
}

pub async fn write(file: &IoFile, data: Vec<u8>, offset: u64) -> RS<usize> {
    #[cfg(target_os = "linux")]
    if crate::io::worker_ring::has_current_worker_ring() {
        return write_submit_option(file, data, offset, OptionWrite::default())?
            .wait()
            .await;
    }

    let len = data.len();
    write_async_portable(file, data, offset).await?;
    Ok(len)
}

pub const fn cloexec_flag() -> i32 {
    #[cfg(unix)]
    {
        libc::O_CLOEXEC
    }
    #[cfg(not(unix))]
    {
        0
    }
}

pub async fn write_option(
    file: &IoFile,
    data: Vec<u8>,
    offset: u64,
    option: OptionWrite,
) -> RS<usize> {
    #[cfg(target_os = "linux")]
    if crate::io::worker_ring::has_current_worker_ring() {
        return write_submit_option(file, data, offset, option)?
            .wait()
            .await;
    }

    let len = data.len();
    let _ = option;
    write_async_portable(file, data, offset).await?;
    Ok(len)
}

pub fn write_submit_fd(fd: RawFd, data: Vec<u8>, offset: u64) -> RS<WriteHandle> {
    write_submit_option_fd(fd, data, offset, OptionWrite::default())
}

pub fn write_submit_option_fd(
    fd: RawFd,
    data: Vec<u8>,
    offset: u64,
    option: OptionWrite,
) -> RS<WriteHandle> {
    #[cfg(target_os = "linux")]
    if crate::io::worker_ring::has_current_worker_ring() {
        return write_submit_option_iouring(fd, data, offset, option);
    }

    let _ = (fd, data, offset, option);
    Err(m_error!(
        EC::NotImplemented,
        "file write submit requires a worker ring; use async write outside io_uring workers"
    ))
}

pub fn write_submit(file: &IoFile, data: Vec<u8>, offset: u64) -> RS<WriteHandle> {
    write_submit_fd(file.fd(), data, offset)
}

pub fn write_submit_option(
    file: &IoFile,
    data: Vec<u8>,
    offset: u64,
    option: OptionWrite,
) -> RS<WriteHandle> {
    write_submit_option_fd(file.fd(), data, offset, option)
}

#[cfg(target_os = "linux")]
fn write_submit_option_iouring(
    fd: RawFd,
    data: Vec<u8>,
    offset: u64,
    option: OptionWrite,
) -> RS<WriteHandle> {
    let total_len = data.len();
    let state = op_state();
    with_current_ring(|ring| {
        ring.register(WorkerRingOp::File(FileIoRequest::Write(
            FileWriteRequest::new(fd, offset, data, option.blind_write, state.clone()),
        )))
        .map(|_| ())
    })?;
    if option.blind_write {
        complete_op(state.clone(), Ok(total_len));
    }
    Ok(WriteHandle { state })
}

pub async fn flush(file: &IoFile) -> RS<()> {
    #[cfg(target_os = "linux")]
    if crate::io::worker_ring::has_current_worker_ring() {
        return flush_submit(file)?.wait().await;
    }

    flush_async_portable(file).await
}

pub async fn flush_lsn(file: &IoFile, ready_lsn: Vec<u32>) -> RS<Vec<u32>> {
    #[cfg(target_os = "linux")]
    if crate::io::worker_ring::has_current_worker_ring() {
        return flush_submit_lsn(file, ready_lsn)?.wait().await;
    }

    flush_async_portable(file).await?;
    Ok(ready_lsn)
}

pub fn flush_submit_fd(fd: RawFd) -> RS<FlushHandle<()>> {
    flush_submit_payload_fd(fd, ())
}

pub fn flush_submit_lsn_fd(fd: RawFd, ready_lsn: Vec<u32>) -> RS<FlushHandle<Vec<u32>>> {
    flush_submit_payload_fd(fd, ready_lsn)
}

pub fn flush_submit_payload_fd<P>(fd: RawFd, payload: P) -> RS<FlushHandle<P>>
where
    P: Send + 'static,
{
    #[cfg(target_os = "linux")]
    if crate::io::worker_ring::has_current_worker_ring() {
        return flush_submit_payload_iouring(fd, payload);
    }

    let _ = (fd, payload);
    Err(m_error!(
        EC::NotImplemented,
        "file flush submit requires a worker ring; use async flush outside io_uring workers"
    ))
}

pub fn flush_submit(file: &IoFile) -> RS<FlushHandle<()>> {
    flush_submit_fd(file.fd())
}

pub fn flush_submit_lsn(file: &IoFile, ready_lsn: Vec<u32>) -> RS<FlushHandle<Vec<u32>>> {
    flush_submit_lsn_fd(file.fd(), ready_lsn)
}

#[cfg(target_os = "linux")]
fn flush_submit_payload_iouring<P>(fd: RawFd, payload: P) -> RS<FlushHandle<P>>
where
    P: Send + 'static,
{
    let state = op_state();
    with_current_ring(|ring| {
        ring.register(WorkerRingOp::File(FileIoRequest::Flush(
            FileFlushRequest::new(fd, payload, state.clone()),
        )))
        .map(|_| ())
    })?;
    Ok(FlushHandle {
        state,
        _marker: std::marker::PhantomData,
    })
}

impl IoFile {
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn from_raw_fd(fd: RawFd) -> Self {
        Self::new(fd)
    }
}

impl WriteHandle {
    pub async fn wait(self) -> RS<usize> {
        poll_fn(|cx| poll_op(&self.state, cx)).await
    }

    pub fn try_take_result(&self) -> Option<RS<usize>> {
        try_take_op(&self.state)
    }
}

impl<P> FlushHandle<P>
where
    P: Send + 'static,
{
    pub async fn wait(self) -> RS<P> {
        poll_fn(|cx| poll_op(&self.state, cx))
            .await
            .and_then(|payload| {
                payload.downcast::<P>().map(|boxed| *boxed).map_err(|_| {
                    mudu::m_error!(EC::InternalErr, "file flush payload type mismatch")
                })
            })
    }

    pub fn try_take_result(&self) -> Option<RS<P>> {
        try_take_op(&self.state).map(|result| {
            result.and_then(|payload| {
                payload.downcast::<P>().map(|boxed| *boxed).map_err(|_| {
                    mudu::m_error!(EC::InternalErr, "file flush payload type mismatch")
                })
            })
        })
    }
}

#[cfg(target_os = "linux")]
impl FileOpenRequest {
    fn new(path: CString, flags: i32, mode: u32, state: Arc<OpState<RawFd>>) -> Self {
        Self {
            path,
            flags,
            mode,
            state,
        }
    }

    pub fn path(&self) -> &CString {
        &self.path
    }

    pub fn flags(&self) -> i32 {
        self.flags
    }

    pub fn mode(&self) -> u32 {
        self.mode
    }

    pub fn finish(self, result: RS<RawFd>) {
        complete_op(self.state, result);
    }
}

#[cfg(target_os = "linux")]
impl FileCloseRequest {
    fn new(fd: RawFd, state: Arc<OpState<()>>) -> Self {
        Self { fd, state }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn finish(self, result: RS<()>) {
        complete_op(self.state, result);
    }
}

#[cfg(target_os = "linux")]
impl FileReadRequest {
    fn new(fd: RawFd, len: usize, offset: u64, state: Arc<OpState<Vec<u8>>>) -> Self {
        Self {
            fd,
            len,
            offset,
            state,
        }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn finish(self, result: RS<Vec<u8>>) {
        complete_op(self.state, result);
    }
}

#[cfg(target_os = "linux")]
impl FileWriteRequest {
    pub(crate) fn new(
        fd: RawFd,
        offset: u64,
        data: Vec<u8>,
        blind_write: bool,
        state: Arc<OpState<usize>>,
    ) -> Self {
        Self {
            fd,
            offset,
            data,
            written: 0,
            blind_write,
            state,
        }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn offset(&self) -> u64 {
        self.offset + self.written as u64
    }

    pub fn data_ptr(&self) -> *const libc::c_void {
        unsafe { self.data.as_ptr().add(self.written) as *const libc::c_void }
    }

    pub fn remaining_len(&self) -> usize {
        self.data.len().saturating_sub(self.written)
    }

    pub fn advance(&mut self, written: usize) {
        self.written += written;
    }

    pub fn is_complete(&self) -> bool {
        self.written >= self.data.len()
    }

    pub fn total_len(&self) -> usize {
        self.data.len()
    }

    pub fn blind_write(&self) -> bool {
        self.blind_write
    }

    pub fn finish(self, result: RS<usize>) {
        complete_op(self.state, result);
    }
}

#[cfg(target_os = "linux")]
impl FileFlushRequest {
    pub fn new<P>(fd: RawFd, payload: P, state: Arc<OpState<Box<dyn Any + Send>>>) -> Self
    where
        P: Send + 'static,
    {
        Self {
            fd,
            payload: Some(Box::new(payload)),
            state,
        }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    fn finish_boxed(self, result: RS<Box<dyn Any + Send>>) {
        complete_op(self.state, result);
    }

    pub fn finish_success(mut self) {
        let payload = self
            .payload
            .take()
            .expect("flush payload must be present when completing");
        self.finish_boxed(Ok(payload));
    }

    pub fn finish_error(self, err: mudu::error::err::MError) {
        self.finish_boxed(Err(err));
    }
}

#[cfg(target_os = "linux")]
impl FileLenRequest {
    pub fn new(fd: RawFd, state: Arc<OpState<u64>>) -> Self {
        Self {
            fd,
            statx: Box::new(unsafe { std::mem::zeroed() }),
            state,
        }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn statx_mut_ptr(&mut self) -> *mut libc::statx {
        self.statx.as_mut()
    }

    pub fn finish(self, result: RS<u64>) {
        complete_op(self.state, result);
    }
}

#[cfg(target_os = "linux")]
enum FileFutureState<T> {
    Init,
    Pending(Arc<OpState<T>>),
    Done,
}

#[cfg(target_os = "linux")]
pub(crate) struct FileOpenFuture {
    path: Option<CString>,
    flags: i32,
    mode: u32,
    state: FileFutureState<RawFd>,
}

#[cfg(target_os = "linux")]
struct FileCloseFuture {
    fd: RawFd,
    state: FileFutureState<()>,
}

#[cfg(target_os = "linux")]
pub(crate) struct FileReadFuture {
    fd: RawFd,
    len: usize,
    offset: u64,
    state: FileFutureState<Vec<u8>>,
}

#[cfg(target_os = "linux")]
pub(crate) struct FileLenFuture {
    fd: RawFd,
    state: FileFutureState<u64>,
}

#[cfg(target_os = "linux")]
impl FileOpenFuture {
    pub(crate) fn new(path: CString, flags: i32, mode: u32) -> Self {
        Self {
            path: Some(path),
            flags,
            mode,
            state: FileFutureState::Init,
        }
    }
}

#[cfg(target_os = "linux")]
impl FileCloseFuture {
    fn new(fd: RawFd) -> Self {
        Self {
            fd,
            state: FileFutureState::Init,
        }
    }
}

#[cfg(target_os = "linux")]
impl FileReadFuture {
    pub(crate) fn new(fd: RawFd, len: usize, offset: u64) -> Self {
        Self {
            fd,
            len,
            offset,
            state: FileFutureState::Init,
        }
    }
}

#[cfg(target_os = "linux")]
impl FileLenFuture {
    pub(crate) fn new(fd: RawFd) -> Self {
        Self {
            fd,
            state: FileFutureState::Init,
        }
    }
}

#[cfg(target_os = "linux")]
impl Future for FileOpenFuture {
    type Output = RS<RawFd>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            FileFutureState::Init => {
                let state = op_state();
                let path = self.path.take().unwrap();
                if let Err(err) = with_current_ring(|ring| {
                    ring.register(WorkerRingOp::File(FileIoRequest::Open(
                        FileOpenRequest::new(path, self.flags, self.mode, state.clone()),
                    )))
                    .map(|_| ())
                }) {
                    self.state = FileFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = FileFutureState::Pending(state);
                self.poll(cx)
            }
            FileFutureState::Pending(state) => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = FileFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            FileFutureState::Done => Poll::Pending,
        }
    }
}

#[cfg(target_os = "linux")]
impl Future for FileCloseFuture {
    type Output = RS<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            FileFutureState::Init => {
                let state = op_state();
                if let Err(err) = with_current_ring(|ring| {
                    ring.register(WorkerRingOp::File(FileIoRequest::Close(
                        FileCloseRequest::new(self.fd, state.clone()),
                    )))
                    .map(|_| ())
                }) {
                    self.state = FileFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = FileFutureState::Pending(state);
                self.poll(cx)
            }
            FileFutureState::Pending(state) => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = FileFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            FileFutureState::Done => Poll::Pending,
        }
    }
}

#[cfg(target_os = "linux")]
impl Future for FileReadFuture {
    type Output = RS<Vec<u8>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            FileFutureState::Init => {
                let state = op_state();
                if let Err(err) = with_current_ring(|ring| {
                    ring.register(WorkerRingOp::File(FileIoRequest::Read(
                        FileReadRequest::new(self.fd, self.len, self.offset, state.clone()),
                    )))
                    .map(|_| ())
                }) {
                    self.state = FileFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = FileFutureState::Pending(state);
                self.poll(cx)
            }
            FileFutureState::Pending(state) => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = FileFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            FileFutureState::Done => Poll::Pending,
        }
    }
}

#[cfg(target_os = "linux")]
impl Future for FileLenFuture {
    type Output = RS<u64>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            FileFutureState::Init => {
                let state = op_state();
                if let Err(err) = with_current_ring(|ring| {
                    ring.register(WorkerRingOp::File(FileIoRequest::Len(
                        FileLenRequest::new(self.fd, state.clone()),
                    )))
                    .map(|_| ())
                }) {
                    self.state = FileFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = FileFutureState::Pending(state);
                self.poll(cx)
            }
            FileFutureState::Pending(state) => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = FileFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            FileFutureState::Done => Poll::Pending,
        }
    }
}

#[cfg(target_os = "linux")]
pub fn submit_file_io(
    request: FileIoRequest,
    sqe: &mut crate::uring::SubmissionQueueEntry<'_>,
) -> FileInflightOp {
    match request {
        FileIoRequest::Open(request) => {
            sqe.prep_openat(
                libc::AT_FDCWD,
                request.path().as_c_str(),
                request.flags(),
                request.mode(),
            );
            FileInflightOp::Open(Box::new(request))
        }
        FileIoRequest::Close(request) => {
            sqe.prep_close(request.fd());
            FileInflightOp::Close(Box::new(request))
        }
        FileIoRequest::Read(request) => {
            let mut buf = vec![0u8; request.len()];
            sqe.prep_read_raw(
                request.fd(),
                buf.as_mut_ptr(),
                request.len(),
                request.offset(),
            );
            FileInflightOp::Read {
                request: Box::new(request),
                buf,
            }
        }
        FileIoRequest::Write(request) => {
            sqe.prep_write_raw(
                request.fd(),
                request.data_ptr().cast(),
                request.remaining_len(),
                request.offset(),
            );
            FileInflightOp::Write(Box::new(request))
        }
        FileIoRequest::Flush(request) => {
            sqe.prep_fsync(request.fd());
            FileInflightOp::Flush(Box::new(request))
        }
        FileIoRequest::Len(mut request) => {
            let empty_path = CStr::from_bytes_with_nul(b"\0").unwrap();
            sqe.prep_statx(
                request.fd(),
                empty_path,
                libc::AT_EMPTY_PATH,
                libc::STATX_SIZE,
                request.statx_mut_ptr(),
            );
            FileInflightOp::Len(Box::new(request))
        }
    }
}

#[cfg(target_os = "linux")]
pub fn complete_file_io(
    op_id: u64,
    op: FileInflightOp,
    result: i32,
    ring: &WorkerLocalRing,
) -> RS<bool> {
    match op {
        FileInflightOp::Open(request) => {
            if result < 0 {
                request.finish(Err(completion_error("file open", result)));
            } else {
                request.finish(Ok(result as RawFd));
            }
            Ok(true)
        }
        FileInflightOp::Close(request) => {
            if result < 0 {
                request.finish(Err(completion_error("file close", result)));
            } else {
                request.finish(Ok(()));
            }
            Ok(true)
        }
        FileInflightOp::Read { request, mut buf } => {
            if result < 0 {
                request.finish(Err(completion_error("file read", result)));
            } else {
                buf.truncate(result as usize);
                request.finish(Ok(buf));
            }
            Ok(true)
        }
        FileInflightOp::Write(mut request) => {
            if result < 0 {
                if !request.blind_write() {
                    request.finish(Err(completion_error("file write", result)));
                }
                Ok(true)
            } else {
                request.advance(result as usize);
                if request.is_complete() {
                    let total = request.total_len();
                    if !request.blind_write() {
                        request.finish(Ok(total));
                    }
                    Ok(true)
                } else {
                    ring.requeue_front(op_id, WorkerRingOp::File(FileIoRequest::Write(*request)))?;
                    Ok(false)
                }
            }
        }
        FileInflightOp::Flush(request) => {
            if result < 0 {
                request.finish_error(completion_error("file flush", result));
            } else {
                request.finish_success();
            }
            Ok(true)
        }
        FileInflightOp::Len(request) => {
            if result < 0 {
                request.finish(Err(completion_error("file len statx", result)));
            } else {
                let len = request.statx.stx_size;
                request.finish(Ok(len));
            }
            Ok(true)
        }
    }
}

async fn open_async_portable(path: &Path, flags: i32, _mode: u32) -> RS<IoFile> {
    let sys_file = crate::io::fs::open(path, flags, _mode).await
        .map_err(|e| m_error!(EC::IOErr, "open file error", e))?;
    sys_file_to_io_file(sys_file)
}

async fn close_async_portable(file: IoFile) -> RS<()> {
    drop(file);
    Ok(())
}

async fn read_async_portable(file: &IoFile, len: usize, offset: u64) -> RS<Vec<u8>> {
    let trace = task_trace!();
    trace.watch("portable_io.op", "read");
    trace.watch("portable_io.offset", &offset.to_string());
    trace.watch("portable_io.len", &len.to_string());
    trace.watch("portable_io.stage", "lock_wait");
    let _guard = file.portable_io_lock.lock().await;
    trace.watch("portable_io.stage", "locked");
    trace.watch("portable_io.stage", "read_start");
    let sys_file = io_file_to_sys_file(file)?;
    let result = crate::io::fs::read_exact_at(&sys_file, len, offset).await
        .map_err(|e| m_error!(EC::IOErr, "read file error", e));
    trace.watch(
        "portable_io.stage",
        if result.is_ok() {
            "read_done"
        } else {
            "read_err"
        },
    );
    result
}

async fn write_async_portable(file: &IoFile, data: Vec<u8>, offset: u64) -> RS<()> {
    let trace = task_trace!();
    trace.watch("portable_io.op", "write");
    trace.watch("portable_io.offset", &offset.to_string());
    trace.watch("portable_io.len", &data.len().to_string());
    trace.watch("portable_io.stage", "lock_wait");
    let _guard = file.portable_io_lock.lock().await;
    trace.watch("portable_io.stage", "locked");
    trace.watch("portable_io.stage", "write_start");
    let sys_file = io_file_to_sys_file(file)?;
    let result = crate::io::fs::write_all_at(&sys_file, &data, offset).await
        .map_err(|e| m_error!(EC::IOErr, "write file error", e));
    trace.watch(
        "portable_io.stage",
        if result.is_ok() {
            "write_done"
        } else {
            "write_err"
        },
    );
    result
}

async fn flush_async_portable(file: &IoFile) -> RS<()> {
    let trace = task_trace!();
    trace.watch("portable_io.op", "flush");
    trace.watch("portable_io.stage", "lock_wait");
    let _guard = file.portable_io_lock.lock().await;
    trace.watch("portable_io.stage", "locked");
    let sys_file = io_file_to_sys_file(file)?;
    let result = crate::io::fs::fsync(&sys_file).await
        .map_err(|e| m_error!(EC::IOErr, "fsync file error", e));
    trace.watch(
        "portable_io.stage",
        if result.is_ok() {
            "flush_done"
        } else {
            "flush_err"
        },
    );
    result
}

fn clone_std_file(file: &IoFile) -> RS<std::fs::File> {
    with_std_file(file, |std_file| {
        std_file
            .try_clone()
            .map_err(|e| m_error!(EC::IOErr, "clone file handle for tokio io error", e))
    })
}

fn sys_file_to_io_file(sys_file: SysFile) -> RS<IoFile> {
    let fd = sys_file.as_raw_fd().ok_or_else(|| {
        m_error!(EC::IOErr, "SysFile does not provide a raw fd")
    })?;
    #[cfg(unix)]
    let new_fd = unsafe { libc::dup(fd) };
    #[cfg(not(unix))]
    let new_fd = fd;
    Ok(IoFile::new(new_fd))
}

fn with_std_file<R>(file: &IoFile, f: impl FnOnce(&std::fs::File) -> RS<R>) -> RS<R> {
    #[cfg(unix)]
    let file = unsafe { ManuallyDrop::new(std::fs::File::from_raw_fd(file.fd())) };
    #[cfg(windows)]
    let file = unsafe { ManuallyDrop::new(std::fs::File::from_raw_handle(file.fd() as RawHandle)) };
    f(&file)
}

fn io_file_to_sys_file(file: &IoFile) -> RS<SysFile> {
    let std_file = clone_std_file(file)?;
    Ok(SysFile::from_std(std_file))
}



#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::io::worker_ring::{set_current_worker_ring, unset_current_worker_ring};
    use crate::tokio::task::yield_now;

    fn install_test_ring() -> Arc<WorkerLocalRing> {
        let ring = Arc::new(WorkerLocalRing::new());
        set_current_worker_ring(ring.clone());
        ring
    }

    #[tokio::test(flavor = "current_thread")]
    async fn open_enqueues_request_and_returns_file() {
        let ring = install_test_ring();
        let task =
            crate::tokio::spawn(async { open("/tmp/test-open", libc::O_RDONLY, 0).await });
        yield_now().await;

        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::File(FileIoRequest::Open(request)) => {
                assert_eq!(request.flags(), libc::O_RDONLY);
                request.finish(Ok(17));
            }
            _ => panic!("expected open request"),
        }

        let file = task.await.unwrap().unwrap();
        assert_eq!(file.fd(), 17);
        unset_current_worker_ring();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn read_enqueues_request_and_receives_payload() {
        let ring = install_test_ring();
        let file = IoFile::new(21);
        let task = crate::tokio::spawn(async move { read(&file, 8, 12).await });
        yield_now().await;

        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::File(FileIoRequest::Read(request)) => {
                assert_eq!(request.fd(), 21);
                assert_eq!(request.len(), 8);
                assert_eq!(request.offset(), 12);
                request.finish(Ok(vec![1, 2, 3]));
            }
            _ => panic!("expected read request"),
        }

        let buf = task.await.unwrap().unwrap();
        assert_eq!(buf, vec![1, 2, 3]);
        unset_current_worker_ring();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn write_flush_and_close_enqueue_requests() {
        let ring = install_test_ring();
        let file = IoFile::new(33);

        let write_task =
            crate::tokio::spawn(async move { write(&file, vec![9, 8, 7], 4).await });
        yield_now().await;
        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::File(FileIoRequest::Write(request)) => {
                assert_eq!(request.fd(), 33);
                assert_eq!(request.offset(), 4);
                assert_eq!(request.remaining_len(), 3);
                request.finish(Ok(3));
            }
            _ => panic!("expected write request"),
        }
        assert_eq!(write_task.await.unwrap().unwrap(), 3);

        let file = IoFile::new(33);
        let flush_task = crate::tokio::spawn(async move { flush(&file).await });
        yield_now().await;
        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::File(FileIoRequest::Flush(request)) => {
                assert_eq!(request.fd(), 33);
                request.finish_success();
            }
            _ => panic!("expected flush request"),
        }
        flush_task.await.unwrap().unwrap();

        let close_task = crate::tokio::spawn(async move { close(IoFile::new(33)).await });
        yield_now().await;
        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::File(FileIoRequest::Close(request)) => {
                assert_eq!(request.fd(), 33);
                request.finish(Ok(()));
            }
            _ => panic!("expected close request"),
        }
        close_task.await.unwrap().unwrap();
        unset_current_worker_ring();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn write_submit_and_wait_split_registration_from_completion() {
        let ring = install_test_ring();
        let file = IoFile::new(44);

        let handle = write_submit(&file, vec![5, 6, 7], 16).unwrap();
        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::File(FileIoRequest::Write(request)) => {
                assert_eq!(request.fd(), 44);
                assert_eq!(request.offset(), 16);
                assert_eq!(request.remaining_len(), 3);
                request.finish(Ok(3));
            }
            _ => panic!("expected write request"),
        }
        assert_eq!(handle.wait().await.unwrap(), 3);
        unset_current_worker_ring();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn blind_write_returns_after_registration() {
        let ring = install_test_ring();
        let file = IoFile::new(55);

        let write_task = crate::tokio::spawn(async move {
            write_option(
                &file,
                vec![1, 2, 3, 4],
                8,
                OptionWrite { blind_write: true },
            )
            .await
        });
        yield_now().await;

        assert!(write_task.is_finished());
        assert_eq!(write_task.await.unwrap().unwrap(), 4);
        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::File(FileIoRequest::Write(request)) => {
                assert_eq!(request.fd(), 55);
                assert_eq!(request.offset(), 8);
                assert!(request.blind_write());
                assert_eq!(request.remaining_len(), 4);
            }
            _ => panic!("expected blind write request"),
        }
        unset_current_worker_ring();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn flush_submit_lsn_and_wait_split_registration_from_completion() {
        let ring = install_test_ring();
        let file = IoFile::new(61);

        let handle = flush_submit_lsn(&file, vec![10, 11]).unwrap();
        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::File(FileIoRequest::Flush(request)) => {
                assert_eq!(request.fd(), 61);
                request.finish_success();
            }
            _ => panic!("expected flush request"),
        }

        assert_eq!(handle.wait().await.unwrap(), vec![10, 11]);
        unset_current_worker_ring();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn flush_lsn_enqueues_request_and_returns_payload() {
        let ring = install_test_ring();
        let file = IoFile::new(41);
        let task = crate::tokio::spawn(async move { flush_lsn(&file, vec![7, 8, 9]).await });
        yield_now().await;

        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::File(FileIoRequest::Flush(request)) => {
                assert_eq!(request.fd(), 41);
                request.finish_success();
            }
            _ => panic!("expected flush request"),
        }

        let ready_lsns = task.await.unwrap().unwrap();
        assert_eq!(ready_lsns, vec![7, 8, 9]);
        unset_current_worker_ring();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn open_without_current_ring_uses_tokio_io() {
        unset_current_worker_ring();
        let path = std::env::temp_dir().join(format!(
            "mudu-portable-file-{}",
            mudu::common::id::gen_oid()
        ));
        let file = open(
            &path,
            libc::O_CREAT | libc::O_TRUNC | libc::O_RDWR | cloexec_flag(),
            0o644,
        )
        .await
        .unwrap();

        assert_eq!(write(&file, b"abcdef".to_vec(), 0).await.unwrap(), 6);
        flush(&file).await.unwrap();
        assert_eq!(read(&file, 3, 2).await.unwrap(), b"cde".to_vec());
        close(file).await.unwrap();

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn file_len_enqueues_request_and_returns_size() {
        let ring = install_test_ring();
        let file = IoFile::new(71);
        let task = crate::tokio::spawn(async move { file_len(&file).await });
        yield_now().await;

        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::File(FileIoRequest::Len(request)) => {
                assert_eq!(request.fd(), 71);
                // Simulate successful statx with size 12345
                let mut request = request;
                unsafe {
                    (*request.statx_mut_ptr()).stx_size = 12345;
                }
                request.finish(Ok(12345));
            }
            _ => panic!("expected file len request"),
        }

        assert_eq!(task.await.unwrap().unwrap(), 12345);
        unset_current_worker_ring();
    }
}

#[cfg(test)]
mod portable_tests {
    use super::*;
    use mudu::common::id::gen_oid;

    #[tokio::test(flavor = "current_thread")]
    async fn portable_async_file_ops_use_tokio_io() {
        let path = std::env::temp_dir().join(format!("mudu-portable-file-{}", gen_oid()));
        let file = open(
            &path,
            libc::O_CREAT | libc::O_TRUNC | libc::O_RDWR | cloexec_flag(),
            0o644,
        )
        .await
        .unwrap();

        assert_eq!(write(&file, b"abcdef".to_vec(), 0).await.unwrap(), 6);
        flush(&file).await.unwrap();
        assert_eq!(read(&file, 3, 2).await.unwrap(), b"cde".to_vec());
        close(file).await.unwrap();

        let _ = std::fs::remove_file(path);
    }
}
