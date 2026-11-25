#![allow(clippy::unwrap_used, clippy::panic)]
use super::*;

#[test]
fn open_registration_is_single_use_and_errors_finish_the_future() {
    let mut future = FileOpenFuture::new(CString::new("/unused").unwrap(), libc::O_RDONLY, 0);
    let request = future.register(op_state()).unwrap();
    assert!(matches!(
        request,
        WorkerRingOp::File(FileIoRequest::Open(_))
    ));
    assert_eq!(
        future.register(op_state()).err().unwrap().ec(),
        ErrorCode::Internal
    );
    let mut cx = Context::from_waker(std::task::Waker::noop());
    let result = Pin::new(&mut future).poll(&mut cx);
    assert!(matches!(result, Poll::Ready(Err(error)) if error.ec() == ErrorCode::Internal));
    assert!(Pin::new(&mut future).poll(&mut cx).is_pending());
}

#[test]
fn completed_open_errors_are_returned_once() {
    let state = op_state();
    complete_op(
        state.clone(),
        Err(mudu_error!(ErrorCode::Io, "open failed")),
    );
    let mut future = FileOpenFuture::new(CString::new("/unused").unwrap(), libc::O_RDONLY, 0);
    *future.state() = FileFutureState::Pending(state);
    let mut cx = Context::from_waker(std::task::Waker::noop());
    assert!(
        matches!(Pin::new(&mut future).poll(&mut cx), Poll::Ready(Err(error)) if error.ec() == ErrorCode::Io)
    );
    assert!(Pin::new(&mut future).poll(&mut cx).is_pending());
}

#[tokio::test]
async fn flush_preserves_failure_and_rejects_wrong_payload_type() {
    let state = op_state();
    let request = FileFlushRequest::new(17, 42u64, state.clone());
    request.finish_error(mudu_error!(ErrorCode::Io, "fsync failed"));
    let handle = FlushHandle::<u64> {
        state,
        _marker: std::marker::PhantomData,
    };
    assert_eq!(handle.wait().await.unwrap_err().ec(), ErrorCode::Io);
    let state = op_state();
    FileFlushRequest::new(17, String::from("wrong payload"), state.clone()).finish_success();
    let handle = FlushHandle::<u64> {
        state,
        _marker: std::marker::PhantomData,
    };
    assert_eq!(handle.wait().await.unwrap_err().ec(), ErrorCode::Internal);
}

#[test]
fn raw_file_wrapper_preserves_descriptor_and_sentinel() {
    assert!(IoFile::from_raw_fd(0).is_invalid());
    let file = IoFile::from_raw_fd(17);
    assert!(!file.is_invalid());
    assert_eq!(file.fd(), 17);
}

#[tokio::test]
async fn kernel_error_completions_reach_all_file_waiters() {
    let ring = WorkerLocalRing::new();
    let state = op_state();
    let request = FileOpenRequest::new(CString::new("/unused").unwrap(), 0, 0, state.clone());
    assert!(complete_file_io(
        1,
        FileInflightOp::Open(Box::new(request)),
        -libc::ENOENT,
        &ring
    )
    .unwrap());
    assert!(wait_op(&state).await.is_err());
    let state = op_state();
    let request = FileCloseRequest::new(-1, state.clone());
    assert!(complete_file_io(
        2,
        FileInflightOp::Close(Box::new(request)),
        -libc::EBADF,
        &ring
    )
    .unwrap());
    assert!(wait_op(&state).await.is_err());
    let state = op_state();
    let request = FileReadRequest::new(-1, 4, 0, state.clone());
    let op = FileInflightOp::Read {
        request: Box::new(request),
        buf: vec![0; 4],
    };
    assert!(complete_file_io(3, op, -libc::EIO, &ring).unwrap());
    assert!(wait_op(&state).await.is_err());
    let state = op_state();
    let request = FileWriteRequest::new(-1, 0, vec![1], false, state.clone());
    assert!(complete_file_io(
        4,
        FileInflightOp::Write(Box::new(request)),
        -libc::ENOSPC,
        &ring
    )
    .unwrap());
    assert!(wait_op(&state).await.is_err());
    let state = op_state();
    let request = FileFlushRequest::new(-1, (), state.clone());
    assert!(complete_file_io(
        5,
        FileInflightOp::Flush(Box::new(request)),
        -libc::EIO,
        &ring
    )
    .unwrap());
    assert!(wait_op(&state).await.is_err());
    let state = op_state();
    let request = FileLenRequest::new(-1, state.clone());
    assert!(complete_file_io(
        6,
        FileInflightOp::Len(Box::new(request)),
        -libc::EBADF,
        &ring
    )
    .unwrap());
    assert!(wait_op(&state).await.is_err());
    assert!(ring.take_pending().unwrap().is_none());
}
