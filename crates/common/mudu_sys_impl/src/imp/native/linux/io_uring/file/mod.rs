#![allow(missing_docs)]
use std::any::Any;
#[cfg(target_os = "linux")]
use std::ffi::CString;
use std::future::poll_fn;
#[cfg(target_os = "linux")]
use std::future::Future;
use std::path::Path;
#[cfg(target_os = "linux")]
use std::pin::Pin;
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::task::{Context, Poll};

#[cfg(target_os = "linux")]
use crate::imp::io::user_io::completion_error;
#[cfg(target_os = "linux")]
use crate::imp::io::user_io::{complete_op, op_state};
use crate::imp::io::user_io::{poll_op, OpState};
#[cfg(target_os = "linux")]
use crate::imp::io::worker_ring::{with_current_ring, WorkerLocalRing, WorkerRingOp};
use crate::{scoped_task_trace, task_trace};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
#[cfg(unix)]
use std::os::fd::RawFd;
#[cfg(windows)]
use std::os::windows::io::RawHandle;

#[cfg(windows)]
type RawFd = usize;

mod close_request;
mod file_close_future;
mod file_future_state;
mod file_inflight_op;
mod file_io_future;
mod file_io_request;
mod file_len_future;
mod file_open_future;
mod file_read_future;
mod flush_handle;
mod flush_request;
mod io_file;
mod len_request;
mod open_request;
mod ops;
mod option_write;
mod read_request;
mod write_handle;
mod write_request;

pub use close_request::FileCloseRequest;
pub use file_inflight_op::FileInflightOp;
pub use file_io_request::FileIoRequest;
pub use flush_handle::FlushHandle;
pub use flush_request::FileFlushRequest;
pub use io_file::IoFile;
pub use len_request::FileLenRequest;
pub use open_request::FileOpenRequest;
pub use option_write::OptionWrite;
pub use read_request::FileReadRequest;
pub use write_handle::WriteHandle;
pub use write_request::FileWriteRequest;

pub(crate) use file_close_future::FileCloseFuture;
pub(crate) use file_future_state::FileFutureState;
pub(crate) use file_io_future::{poll_file_io_future, FileIoFuture};
pub(crate) use file_len_future::FileLenFuture;
pub(crate) use file_open_future::FileOpenFuture;
pub(crate) use file_read_future::FileReadFuture;
pub(crate) use ops::{complete_file_io, submit_file_io};

async fn wait_op<T>(state: &Arc<OpState<T>>) -> RS<T> {
    poll_fn(|cx| poll_op(state, cx)).await
}

#[cfg(target_os = "linux")]
pub async fn open<P: AsRef<Path>>(path: P, flags: i32, mode: u32) -> RS<IoFile> {
    scoped_task_trace!();
    assert_worker_ring()?;
    let path = CString::new(path.as_ref().as_os_str().as_encoded_bytes())
        .map_err(|_| mudu_error!(ErrorCode::PathContainsNul, "path contains NUL byte"))?;
    let fd = FileOpenFuture::new(path, flags, mode).await?;
    Ok(IoFile::from_raw_fd(fd))
}

#[cfg(target_os = "linux")]
pub async fn close(file: IoFile) -> RS<()> {
    assert_worker_ring()?;
    FileCloseFuture::new(file.fd).await
}

#[cfg(target_os = "linux")]
pub async fn read(file: &IoFile, len: usize, offset: u64) -> RS<Vec<u8>> {
    assert_worker_ring()?;
    FileReadFuture::new(file.fd, len, offset).await
}

#[cfg(target_os = "linux")]
pub async fn file_len(file: &IoFile) -> RS<u64> {
    assert_worker_ring()?;
    FileLenFuture::new(file.fd).await
}

pub async fn metadata_len<P: AsRef<Path>>(path: P) -> RS<u64> {
    crate::fs::async_::metadata_len(path.as_ref()).await
}

#[cfg(target_os = "linux")]
pub async fn write(file: &IoFile, data: Vec<u8>, offset: u64) -> RS<usize> {
    assert_worker_ring()?;
    write_submit_option(file, data, offset, OptionWrite::default())?
        .wait()
        .await
}

#[cfg(target_os = "linux")]
pub async fn write_option(
    file: &IoFile,
    data: Vec<u8>,
    offset: u64,
    option: OptionWrite,
) -> RS<usize> {
    assert_worker_ring()?;
    write_submit_option(file, data, offset, option)?
        .wait()
        .await
}

#[cfg(target_os = "linux")]
pub fn write_submit_fd(fd: RawFd, data: Vec<u8>, offset: u64) -> RS<WriteHandle> {
    write_submit_option_fd(fd, data, offset, OptionWrite::default())
}

#[cfg(target_os = "linux")]
pub fn write_submit_option_fd(
    fd: RawFd,
    data: Vec<u8>,
    offset: u64,
    option: OptionWrite,
) -> RS<WriteHandle> {
    assert_worker_ring()?;
    write_submit_option_iouring(fd, data, offset, option)
}

#[cfg(target_os = "linux")]
pub fn write_submit(file: &IoFile, data: Vec<u8>, offset: u64) -> RS<WriteHandle> {
    write_submit_fd(file.fd(), data, offset)
}

#[cfg(target_os = "linux")]
pub fn write_submit_option(
    file: &IoFile,
    data: Vec<u8>,
    offset: u64,
    option: OptionWrite,
) -> RS<WriteHandle> {
    write_submit_option_fd(file.fd(), data, offset, option)
}

#[cfg(target_os = "linux")]
fn assert_worker_ring() -> RS<()> {
    if !crate::imp::io::worker_ring::has_current_worker_ring() {
        return Err(mudu_error!(
            ErrorCode::Internal,
            "IoFile operation requires a current worker ring"
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn register_file_op(op: WorkerRingOp) -> RS<()> {
    with_current_ring(|ring| ring.register(op).map(|_| ()))
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
    register_file_op(WorkerRingOp::File(FileIoRequest::Write(
        FileWriteRequest::new(fd, offset, data, option.blind_write, state.clone()),
    )))?;
    if option.blind_write {
        complete_op(state.clone(), Ok(total_len));
    }
    Ok(WriteHandle { state })
}

#[cfg(target_os = "linux")]
pub async fn flush(file: &IoFile) -> RS<()> {
    assert_worker_ring()?;
    flush_submit(file)?.wait().await
}

#[cfg(target_os = "linux")]
pub async fn flush_lsn(file: &IoFile, ready_lsn: Vec<u64>) -> RS<Vec<u64>> {
    assert_worker_ring()?;
    flush_submit_lsn(file, ready_lsn)?.wait().await
}

#[cfg(target_os = "linux")]
pub fn flush_submit_fd(fd: RawFd) -> RS<FlushHandle<()>> {
    flush_submit_payload_fd(fd, ())
}

#[cfg(target_os = "linux")]
pub fn flush_submit_lsn_fd(fd: RawFd, ready_lsn: Vec<u64>) -> RS<FlushHandle<Vec<u64>>> {
    flush_submit_payload_fd(fd, ready_lsn)
}

#[cfg(target_os = "linux")]
pub fn flush_submit_payload_fd<P>(fd: RawFd, payload: P) -> RS<FlushHandle<P>>
where
    P: Send + 'static,
{
    assert_worker_ring()?;
    flush_submit_payload_iouring(fd, payload)
}

#[cfg(target_os = "linux")]
pub fn flush_submit(file: &IoFile) -> RS<FlushHandle<()>> {
    flush_submit_fd(file.fd())
}

#[cfg(target_os = "linux")]
pub fn flush_submit_lsn(file: &IoFile, ready_lsn: Vec<u64>) -> RS<FlushHandle<Vec<u64>>> {
    flush_submit_lsn_fd(file.fd(), ready_lsn)
}

#[cfg(target_os = "linux")]
fn flush_submit_payload_iouring<P>(fd: RawFd, payload: P) -> RS<FlushHandle<P>>
where
    P: Send + 'static,
{
    let state = op_state();
    register_file_op(WorkerRingOp::File(FileIoRequest::Flush(
        FileFlushRequest::new(fd, payload, state.clone()),
    )))?;
    Ok(FlushHandle {
        state,
        _marker: std::marker::PhantomData,
    })
}

#[cfg(all(test, target_os = "linux"))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::imp::io::worker_ring::{set_current_worker_ring, unset_current_worker_ring};
    use crate::task::async_::spawn_task_detached;
    use tokio::task::yield_now;

    #[allow(clippy::arc_with_non_send_sync)]
    fn install_test_ring() -> Arc<WorkerLocalRing> {
        let ring = Arc::new(WorkerLocalRing::new());
        set_current_worker_ring(ring.clone());
        ring
    }

    #[tokio::test(flavor = "current_thread")]
    async fn open_enqueues_request_and_returns_file() {
        let ring = install_test_ring();
        let task = spawn_task_detached("test", async {
            open("/tmp/test-open", libc::O_RDONLY, 0).await
        })
        .unwrap();
        yield_now().await;

        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::File(FileIoRequest::Open(request)) => {
                assert_eq!(request.flags(), libc::O_RDONLY);
                request.finish(Ok(17));
            }
            _ => panic!("expected open request"),
        }

        let file = task.await.unwrap().unwrap().unwrap();
        assert_eq!(file.fd(), 17);
        unset_current_worker_ring();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn read_enqueues_request_and_receives_payload() {
        let ring = install_test_ring();
        let file = IoFile::new(21);
        let task = spawn_task_detached("test", async move { read(&file, 8, 12).await }).unwrap();
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

        let buf = task.await.unwrap().unwrap().unwrap();
        assert_eq!(buf, vec![1, 2, 3]);
        unset_current_worker_ring();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn write_flush_and_close_enqueue_requests() {
        let ring = install_test_ring();
        let file = IoFile::new(33);

        let write_task =
            spawn_task_detached("test", async move { write(&file, vec![9, 8, 7], 4).await })
                .unwrap();
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
        assert_eq!(write_task.await.unwrap().unwrap().unwrap(), 3);

        let file = IoFile::new(33);
        let flush_task = spawn_task_detached("test", async move { flush(&file).await }).unwrap();
        yield_now().await;
        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::File(FileIoRequest::Flush(request)) => {
                assert_eq!(request.fd(), 33);
                request.finish_success();
            }
            _ => panic!("expected flush request"),
        }
        flush_task.await.unwrap().unwrap().unwrap();

        let close_task =
            spawn_task_detached("test", async move { close(IoFile::new(33)).await }).unwrap();
        yield_now().await;
        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::File(FileIoRequest::Close(request)) => {
                assert_eq!(request.fd(), 33);
                request.finish(Ok(()));
            }
            _ => panic!("expected close request"),
        }
        close_task.await.unwrap().unwrap().unwrap();
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

        let write_task = spawn_task_detached("test", async move {
            write_option(
                &file,
                vec![1, 2, 3, 4],
                8,
                OptionWrite { blind_write: true },
            )
            .await
        })
        .unwrap();
        yield_now().await;

        assert!(write_task.is_finished());
        assert_eq!(write_task.await.unwrap().unwrap().unwrap(), 4);
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
        let task =
            spawn_task_detached("test", async move { flush_lsn(&file, vec![7, 8, 9]).await })
                .unwrap();
        yield_now().await;

        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::File(FileIoRequest::Flush(request)) => {
                assert_eq!(request.fd(), 41);
                request.finish_success();
            }
            _ => panic!("expected flush request"),
        }

        let ready_lsns = task.await.unwrap().unwrap().unwrap();
        assert_eq!(ready_lsns, vec![7, 8, 9]);
        unset_current_worker_ring();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn open_without_current_ring_returns_error() {
        unset_current_worker_ring();
        let path = std::env::temp_dir().join(format!(
            "mudu-no-ring-file-{}",
            crate::random::uuid_v4().as_u128()
        ));
        let result = open(
            &path,
            libc::O_CREAT | libc::O_TRUNC | libc::O_RDWR | libc::O_CLOEXEC,
            0o644,
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn file_len_enqueues_request_and_returns_size() {
        let ring = install_test_ring();
        let file = IoFile::new(71);
        let task = spawn_task_detached("test", async move { file_len(&file).await }).unwrap();
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

        assert_eq!(task.await.unwrap().unwrap().unwrap(), 12345);
        unset_current_worker_ring();
    }
}
