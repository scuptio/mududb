use std::future::Future;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::os::fd::RawFd;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

use crate::imp::io::user_io::{op_state, poll_op, OpState};
use crate::imp::native::linux::io_uring::worker_ring::{with_current_ring, WorkerRingOp};

use super::request::{
    SocketAcceptRequest, SocketCloseRequest, SocketConnectRequest, SocketIoRequest,
    SocketOpenRequest, SocketRecvRequest, SocketSendRefRequest, SocketSendRequest,
    SocketShutdownRequest,
};

enum SocketFutureState<T> {
    Init,
    Pending(Arc<OpState<T>>),
    Done,
}

pub(super) struct SocketOpenFuture {
    domain: i32,
    socket_type: i32,
    protocol: i32,
    state: SocketFutureState<RawFd>,
}

pub(super) struct SocketConnectFuture {
    fd: RawFd,
    addr: Option<crate::imp::native::linux::io_uring::iouring::SockAddrBuf>,
    state: SocketFutureState<()>,
}

pub(super) struct SocketAcceptFuture {
    fd: RawFd,
    state: SocketFutureState<(RawFd, SocketAddr)>,
}

pub(super) struct SocketRecvIntoFuture<'a> {
    fd: RawFd,
    buf_ptr: *mut u8,
    len: usize,
    flags: i32,
    state: SocketFutureState<usize>,
    _marker: PhantomData<&'a mut [u8]>,
}

unsafe impl<'a> Send for SocketRecvIntoFuture<'a> {}

pub(super) struct SocketSendFuture {
    fd: RawFd,
    data: Option<Vec<u8>>,
    flags: i32,
    state: SocketFutureState<usize>,
}

pub(super) struct SocketSendRefFuture<'a> {
    fd: RawFd,
    data_ptr: *const u8,
    len: usize,
    flags: i32,
    state: SocketFutureState<usize>,
    _marker: PhantomData<&'a [u8]>,
}

pub(super) struct SocketShutdownFuture {
    fd: RawFd,
    how: i32,
    state: SocketFutureState<()>,
}

pub(super) struct SocketCloseFuture {
    fd: RawFd,
    state: SocketFutureState<()>,
}

impl SocketOpenFuture {
    pub(super) fn new(domain: i32, socket_type: i32, protocol: i32) -> Self {
        Self {
            domain,
            socket_type,
            protocol,
            state: SocketFutureState::Init,
        }
    }
}

impl SocketConnectFuture {
    pub(super) fn new(
        fd: RawFd,
        addr: crate::imp::native::linux::io_uring::iouring::SockAddrBuf,
    ) -> Self {
        Self {
            fd,
            addr: Some(addr),
            state: SocketFutureState::Init,
        }
    }
}

impl SocketAcceptFuture {
    pub(super) fn new(fd: RawFd) -> Self {
        Self {
            fd,
            state: SocketFutureState::Init,
        }
    }
}

impl<'a> SocketRecvIntoFuture<'a> {
    pub(super) fn new(fd: RawFd, buf: &'a mut [u8], flags: i32) -> Self {
        Self {
            fd,
            buf_ptr: buf.as_mut_ptr(),
            len: buf.len(),
            flags,
            state: SocketFutureState::Init,
            _marker: PhantomData,
        }
    }
}

impl SocketSendFuture {
    pub(super) fn new(fd: RawFd, data: Vec<u8>, flags: i32) -> Self {
        Self {
            fd,
            data: Some(data),
            flags,
            state: SocketFutureState::Init,
        }
    }
}

impl<'a> SocketSendRefFuture<'a> {
    pub(super) fn new(fd: RawFd, data: &'a [u8], flags: i32) -> Self {
        Self {
            fd,
            data_ptr: data.as_ptr(),
            len: data.len(),
            flags,
            state: SocketFutureState::Init,
            _marker: PhantomData,
        }
    }
}

impl SocketShutdownFuture {
    pub(super) fn new(fd: RawFd, how: i32) -> Self {
        Self {
            fd,
            how,
            state: SocketFutureState::Init,
        }
    }
}

impl SocketCloseFuture {
    pub(super) fn new(fd: RawFd) -> Self {
        Self {
            fd,
            state: SocketFutureState::Init,
        }
    }
}

impl Future for SocketOpenFuture {
    type Output = RS<RawFd>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            SocketFutureState::Init => {
                let state = op_state();
                if let Err(err) = with_current_ring(|ring| {
                    ring.register(WorkerRingOp::Socket(SocketIoRequest::Socket(
                        SocketOpenRequest::new(
                            self.domain,
                            self.socket_type,
                            self.protocol,
                            state.clone(),
                        ),
                    )))
                    .map(|_| ())
                }) {
                    self.state = SocketFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = SocketFutureState::Pending(state);
                self.poll(cx)
            }
            SocketFutureState::Pending(state) => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = SocketFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            SocketFutureState::Done => Poll::Pending,
        }
    }
}

impl Future for SocketConnectFuture {
    type Output = RS<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            SocketFutureState::Init => {
                let state = op_state();
                let Some(addr) = self.addr.take() else {
                    self.state = SocketFutureState::Done;
                    return Poll::Ready(Err(mudu_error!(
                        ErrorCode::Internal,
                        "SocketConnectFuture polled after completion"
                    )));
                };
                if let Err(err) = with_current_ring(|ring| {
                    ring.register(WorkerRingOp::Socket(SocketIoRequest::Connect(
                        SocketConnectRequest::new(self.fd, addr, state.clone()),
                    )))
                    .map(|_| ())
                }) {
                    self.state = SocketFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = SocketFutureState::Pending(state);
                self.poll(cx)
            }
            SocketFutureState::Pending(state) => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = SocketFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            SocketFutureState::Done => Poll::Pending,
        }
    }
}

impl Future for SocketAcceptFuture {
    type Output = RS<(RawFd, SocketAddr)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            SocketFutureState::Init => {
                let state = op_state();
                if let Err(err) = with_current_ring(|ring| {
                    ring.register(WorkerRingOp::Socket(SocketIoRequest::Accept(
                        SocketAcceptRequest::new(self.fd, state.clone()),
                    )))
                    .map(|_| ())
                }) {
                    self.state = SocketFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = SocketFutureState::Pending(state);
                self.poll(cx)
            }
            SocketFutureState::Pending(state) => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = SocketFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            SocketFutureState::Done => Poll::Pending,
        }
    }
}

impl<'a> Future for SocketRecvIntoFuture<'a> {
    type Output = RS<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            SocketFutureState::Init => {
                let state = op_state();
                if let Err(err) = with_current_ring(|ring| {
                    ring.register(WorkerRingOp::Socket(SocketIoRequest::Recv(
                        SocketRecvRequest::new(
                            self.fd,
                            self.buf_ptr,
                            self.len,
                            self.flags,
                            state.clone(),
                        ),
                    )))
                    .map(|_| ())
                }) {
                    self.state = SocketFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = SocketFutureState::Pending(state);
                self.poll(cx)
            }
            SocketFutureState::Pending(state) => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = SocketFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            SocketFutureState::Done => Poll::Pending,
        }
    }
}

impl Future for SocketSendFuture {
    type Output = RS<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            SocketFutureState::Init => {
                let state = op_state();
                let Some(data) = self.data.take() else {
                    self.state = SocketFutureState::Done;
                    return Poll::Ready(Err(mudu_error!(
                        ErrorCode::Internal,
                        "SocketSendFuture polled after completion"
                    )));
                };
                if let Err(err) = with_current_ring(|ring| {
                    ring.register(WorkerRingOp::Socket(SocketIoRequest::Send(
                        SocketSendRequest::new(self.fd, data, self.flags, state.clone()),
                    )))
                    .map(|_| ())
                }) {
                    self.state = SocketFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = SocketFutureState::Pending(state);
                self.poll(cx)
            }
            SocketFutureState::Pending(state) => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = SocketFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            SocketFutureState::Done => Poll::Pending,
        }
    }
}

unsafe impl<'a> Send for SocketSendRefFuture<'a> {}

impl<'a> Future for SocketSendRefFuture<'a> {
    type Output = RS<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            SocketFutureState::Init => {
                let state = op_state();
                if let Err(err) = with_current_ring(|ring| {
                    ring.register(WorkerRingOp::Socket(SocketIoRequest::SendRef(
                        SocketSendRefRequest::new(
                            self.fd,
                            self.data_ptr,
                            self.len,
                            self.flags,
                            state.clone(),
                        ),
                    )))
                    .map(|_| ())
                }) {
                    self.state = SocketFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = SocketFutureState::Pending(state);
                self.poll(cx)
            }
            SocketFutureState::Pending(state) => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = SocketFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            SocketFutureState::Done => Poll::Pending,
        }
    }
}

impl Future for SocketShutdownFuture {
    type Output = RS<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            SocketFutureState::Init => {
                let state = op_state();
                if let Err(err) = with_current_ring(|ring| {
                    ring.register(WorkerRingOp::Socket(SocketIoRequest::Shutdown(
                        SocketShutdownRequest::new(self.fd, self.how, state.clone()),
                    )))
                    .map(|_| ())
                }) {
                    self.state = SocketFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = SocketFutureState::Pending(state);
                self.poll(cx)
            }
            SocketFutureState::Pending(state) => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = SocketFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            SocketFutureState::Done => Poll::Pending,
        }
    }
}

impl Future for SocketCloseFuture {
    type Output = RS<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &self.state {
            SocketFutureState::Init => {
                let state = op_state();
                if let Err(err) = with_current_ring(|ring| {
                    ring.register(WorkerRingOp::Socket(SocketIoRequest::Close(
                        SocketCloseRequest::new(self.fd, state.clone()),
                    )))
                    .map(|_| ())
                }) {
                    self.state = SocketFutureState::Done;
                    return Poll::Ready(Err(err));
                }
                self.state = SocketFutureState::Pending(state);
                self.poll(cx)
            }
            SocketFutureState::Pending(state) => match poll_op(state, cx) {
                Poll::Ready(result) => {
                    self.state = SocketFutureState::Done;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Pending,
            },
            SocketFutureState::Done => Poll::Pending,
        }
    }
}
