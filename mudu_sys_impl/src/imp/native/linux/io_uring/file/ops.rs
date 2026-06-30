use super::*;

pub(crate) fn submit_file_io(
    request: FileIoRequest,
    sqe: &mut crate::imp::native::linux::io_uring::iouring::SubmissionQueueEntry<'_>,
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
            let empty_path = c"";
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
pub(crate) fn complete_file_io(
    op_id: u64,
    op: FileInflightOp,
    result: i32,
    ring: &WorkerLocalRing,
) -> RS<bool> {
    match op {
        FileInflightOp::Open(request) => {
            tracing::debug!(op_id, result, "file io complete open");
            if result < 0 {
                request.finish(Err(completion_error("file open", result)));
            } else {
                request.finish(Ok(result as RawFd));
            }
            Ok(true)
        }
        FileInflightOp::Close(request) => {
            tracing::debug!(op_id, result, "file io complete close");
            if result < 0 {
                request.finish(Err(completion_error("file close", result)));
            } else {
                request.finish(Ok(()));
            }
            Ok(true)
        }
        FileInflightOp::Read { request, mut buf } => {
            tracing::debug!(
                op_id,
                result,
                requested_len = buf.len(),
                "file io complete read"
            );
            if result < 0 {
                request.finish(Err(completion_error("file read", result)));
            } else {
                buf.truncate(result as usize);
                request.finish(Ok(buf));
            }
            Ok(true)
        }
        FileInflightOp::Write(mut request) => {
            tracing::debug!(op_id, result, "file io complete write");
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
            tracing::debug!(op_id, result, "file io complete flush");
            if result < 0 {
                request.finish_error(completion_error("file flush", result));
            } else {
                request.finish_success();
            }
            Ok(true)
        }
        FileInflightOp::Len(request) => {
            tracing::debug!(op_id, result, "file io complete len");
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
