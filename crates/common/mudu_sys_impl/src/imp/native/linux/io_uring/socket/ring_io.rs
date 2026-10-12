use std::os::fd::RawFd;

use mudu::common::result::RS;

use crate::imp::io::user_io::completion_error;
use crate::imp::native::linux::io_uring::worker_ring::{WorkerLocalRing, WorkerRingOp};

use super::raw_to_socket_addr;
use super::request::{SocketInflightOp, SocketIoRequest};

/// Prepares a socket I/O SQE and returns the corresponding inflight op.
///
/// # Safety warning
/// Several io_uring prep functions (e.g. `prep_connect`, `prep_accept`)
/// store a raw pointer to data inside the SQE. Because the request is moved
/// into a `Box` for the inflight op, any pointer captured while the request
/// is still on the stack will become dangling. Always `Box::new(request)`
/// **before** calling any prep function that stores pointers.
pub fn submit_socket_io(
    request: SocketIoRequest,
    sqe: &mut crate::imp::native::linux::io_uring::iouring::SubmissionQueueEntry<'_>,
) -> SocketInflightOp {
    match request {
        SocketIoRequest::Socket(request) => {
            sqe.prep_socket(
                request.domain(),
                request.socket_type(),
                request.protocol(),
                0,
            );
            SocketInflightOp::Open(Box::new(request))
        }
        SocketIoRequest::Connect(request) => {
            // IMPORTANT: Box the request BEFORE calling prep_connect.
            // io_uring_prep_connect stores a raw pointer to the sockaddr inside
            // the SQE. If we pass request.addr() while request is still on the
            // stack, the pointer becomes dangling as soon as Box::new(request)
            // moves the SockAddrBuf to the heap. The kernel then reads garbage
            // (e.g. wrong address family), which caused EAFNOSUPPORT (-97).
            // Always box first so the pointer in the SQE points to stable heap
            // memory that stays valid until the CQE is processed.
            let request = Box::new(request);
            sqe.prep_connect(request.fd(), request.addr());
            SocketInflightOp::Connect(request)
        }
        SocketIoRequest::Accept(request) => {
            let mut request = Box::new(request);
            sqe.prep_accept(request.fd(), request.addr_mut(), 0);
            SocketInflightOp::Accept(request)
        }
        SocketIoRequest::Recv(request) => {
            sqe.prep_recv_raw(
                request.fd(),
                request.buf_ptr().cast(),
                request.len(),
                request.flags(),
            );
            SocketInflightOp::Recv(Box::new(request))
        }
        SocketIoRequest::Send(request) => {
            sqe.prep_send_raw(
                request.fd(),
                request.data_ptr().cast(),
                request.remaining_len(),
                request.flags(),
            );
            SocketInflightOp::Send(Box::new(request))
        }
        SocketIoRequest::SendRef(request) => {
            sqe.prep_send_raw(
                request.fd(),
                request.data_ptr().cast(),
                request.remaining_len(),
                request.flags(),
            );
            SocketInflightOp::SendRef(Box::new(request))
        }
        SocketIoRequest::Shutdown(request) => {
            sqe.prep_shutdown(request.fd(), request.how());
            SocketInflightOp::Shutdown(Box::new(request))
        }
        SocketIoRequest::Close(request) => {
            sqe.prep_close(request.fd());
            SocketInflightOp::Close(Box::new(request))
        }
    }
}

/// Completes an inflight socket operation from its CQE result, delivering
/// the outcome to the waiting task. Returns `Ok(true)` when the op is done;
/// a short send requeues the remainder on `ring` and returns `Ok(false)`.
pub fn complete_socket_io(
    op_id: u64,
    op: SocketInflightOp,
    result: i32,
    ring: &WorkerLocalRing,
) -> RS<bool> {
    match op {
        SocketInflightOp::Open(request) => {
            if result < 0 {
                request.finish(Err(completion_error("socket open", result)));
            } else {
                request.finish(Ok(result as RawFd));
            }
            Ok(true)
        }
        SocketInflightOp::Connect(request) => {
            if result < 0 {
                request.finish(Err(completion_error("socket connect", result)));
            } else {
                request.finish(Ok(()));
            }
            Ok(true)
        }
        SocketInflightOp::Accept(request) => {
            if result < 0 {
                request.finish(Err(completion_error("socket accept", result)));
            } else {
                let remote_addr = raw_to_socket_addr(request.addr())?;
                request.finish(Ok((result as RawFd, remote_addr)));
            }
            Ok(true)
        }
        SocketInflightOp::Recv(request) => {
            if result < 0 {
                request.finish(Err(completion_error("socket recv", result)));
            } else {
                request.finish(Ok(result as usize));
            }
            Ok(true)
        }
        SocketInflightOp::Send(mut request) => {
            if result < 0 {
                request.finish(Err(completion_error("socket send", result)));
                Ok(true)
            } else {
                request.advance(result as usize);
                if request.is_complete() {
                    let total = request.total_len();
                    request.finish(Ok(total));
                    Ok(true)
                } else {
                    ring.requeue_front(
                        op_id,
                        WorkerRingOp::Socket(SocketIoRequest::Send(*request)),
                    )?;
                    Ok(false)
                }
            }
        }
        SocketInflightOp::SendRef(mut request) => {
            if result < 0 {
                request.finish(Err(completion_error("socket send", result)));
                Ok(true)
            } else {
                request.advance(result as usize);
                if request.is_complete() {
                    let total = request.total_len();
                    request.finish(Ok(total));
                    Ok(true)
                } else {
                    ring.requeue_front(
                        op_id,
                        WorkerRingOp::Socket(SocketIoRequest::SendRef(*request)),
                    )?;
                    Ok(false)
                }
            }
        }
        SocketInflightOp::Shutdown(request) => {
            if result < 0 {
                request.finish(Err(completion_error("socket shutdown", result)));
            } else {
                request.finish(Ok(()));
            }
            Ok(true)
        }
        SocketInflightOp::Close(request) => {
            if result < 0 {
                request.finish(Err(completion_error("socket close", result)));
            } else {
                request.finish(Ok(()));
            }
            Ok(true)
        }
    }
}
