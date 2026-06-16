use std::future::Future;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::os::fd::RawFd;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;

use crate::imp::native::linux::io_uring::worker_ring::{
    WorkerLocalRing, WorkerRingOp, with_current_ring,
};
use crate::io::user_io::{OpState, complete_op, completion_error, op_state, poll_op};

#[derive(Debug)]
pub struct IoSocket {
    fd: RawFd,
}

pub enum SocketIoRequest {
    Socket(SocketOpenRequest),
    Connect(SocketConnectRequest),
    Accept(SocketAcceptRequest),
    Recv(SocketRecvRequest),
    Send(SocketSendRequest),
    SendRef(SocketSendRefRequest),
    Shutdown(SocketShutdownRequest),
    Close(SocketCloseRequest),
}

pub enum SocketInflightOp {
    Open(Box<SocketOpenRequest>),
    Connect(Box<SocketConnectRequest>),
    Accept(Box<SocketAcceptRequest>),
    Recv(Box<SocketRecvRequest>),
    Send(Box<SocketSendRequest>),
    SendRef(Box<SocketSendRefRequest>),
    Shutdown(Box<SocketShutdownRequest>),
    Close(Box<SocketCloseRequest>),
}

pub struct SocketOpenRequest {
    domain: i32,
    socket_type: i32,
    protocol: i32,
    state: Arc<OpState<RawFd>>,
}

pub struct SocketConnectRequest {
    fd: RawFd,
    addr: crate::imp::native::linux::io_uring::iouring::SockAddrBuf,
    state: Arc<OpState<()>>,
}

pub struct SocketAcceptRequest {
    fd: RawFd,
    addr: crate::imp::native::linux::io_uring::iouring::SockAddrBuf,
    state: Arc<OpState<(RawFd, SocketAddr)>>,
}

pub struct SocketRecvRequest {
    fd: RawFd,
    buf_ptr: *mut u8,
    len: usize,
    flags: i32,
    state: Arc<OpState<usize>>,
}

pub struct SocketSendRequest {
    fd: RawFd,
    flags: i32,
    data: Vec<u8>,
    sent: usize,
    state: Arc<OpState<usize>>,
}

pub struct SocketSendRefRequest {
    fd: RawFd,
    flags: i32,
    data_ptr: *const u8,
    len: usize,
    sent: usize,
    state: Arc<OpState<usize>>,
}

pub struct SocketShutdownRequest {
    fd: RawFd,
    how: i32,
    state: Arc<OpState<()>>,
}

pub struct SocketCloseRequest {
    fd: RawFd,
    state: Arc<OpState<()>>,
}

pub async fn socket(domain: i32, socket_type: i32, protocol: i32) -> RS<IoSocket> {
    let fd = SocketOpenFuture::new(domain, socket_type, protocol).await?;
    Ok(IoSocket { fd })
}

pub async fn connect(sock: &IoSocket, addr: SocketAddr) -> RS<()> {
    SocketConnectFuture::new(sock.fd, addr).await
}

pub async fn accept(sock: &IoSocket) -> RS<(IoSocket, SocketAddr)> {
    let (fd, addr) = SocketAcceptFuture::new(sock.fd).await?;
    Ok((IoSocket { fd }, addr))
}

pub async fn recv(sock: &IoSocket, len: usize, flags: i32) -> RS<Vec<u8>> {
    let mut buf = vec![0u8; len];
    let read = recv_into(sock, buf.as_mut_slice(), flags).await?;
    buf.truncate(read);
    Ok(buf)
}

pub async fn send(sock: &IoSocket, data: Vec<u8>, flags: i32) -> RS<usize> {
    SocketSendFuture::new(sock.fd, data, flags).await
}

pub async fn shutdown(sock: &IoSocket, how: i32) -> RS<()> {
    SocketShutdownFuture::new(sock.fd, how).await
}

pub async fn close(sock: IoSocket) -> RS<()> {
    SocketCloseFuture::new(sock.fd).await
}

pub async fn recv_into(sock: &IoSocket, buf: &mut [u8], flags: i32) -> RS<usize> {
    SocketRecvIntoFuture::new(sock.fd, buf, flags).await
}

pub async fn send_all(sock: &IoSocket, data: &[u8]) -> RS<()> {
    let sent = SocketSendRefFuture::new(sock.fd, data, 0).await?;
    if sent != data.len() {
        return Err(m_error!(
            EC::NetErr,
            format!(
                "socket send incomplete: sent {}, expected {}",
                sent,
                data.len()
            )
        ));
    }
    Ok(())
}

impl IoSocket {
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd }
    }
}

impl SocketOpenRequest {
    fn new(domain: i32, socket_type: i32, protocol: i32, state: Arc<OpState<RawFd>>) -> Self {
        Self {
            domain,
            socket_type,
            protocol,
            state,
        }
    }

    pub fn domain(&self) -> i32 {
        self.domain
    }

    pub fn socket_type(&self) -> i32 {
        self.socket_type
    }

    pub fn protocol(&self) -> i32 {
        self.protocol
    }

    pub fn finish(self, result: RS<RawFd>) {
        complete_op(self.state, result);
    }
}

impl SocketConnectRequest {
    fn new(
        fd: RawFd,
        addr: crate::imp::native::linux::io_uring::iouring::SockAddrBuf,
        state: Arc<OpState<()>>,
    ) -> Self {
        Self { fd, addr, state }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn addr(&self) -> &crate::imp::native::linux::io_uring::iouring::SockAddrBuf {
        &self.addr
    }

    pub fn finish(self, result: RS<()>) {
        complete_op(self.state, result);
    }
}

impl SocketAcceptRequest {
    fn new(fd: RawFd, state: Arc<OpState<(RawFd, SocketAddr)>>) -> Self {
        Self {
            fd,
            addr: crate::imp::native::linux::io_uring::iouring::SockAddrBuf::new_empty(),
            state,
        }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn addr_mut(&mut self) -> &mut crate::imp::native::linux::io_uring::iouring::SockAddrBuf {
        &mut self.addr
    }

    pub fn addr(&self) -> &crate::imp::native::linux::io_uring::iouring::SockAddrBuf {
        &self.addr
    }

    pub fn finish(self, result: RS<(RawFd, SocketAddr)>) {
        complete_op(self.state, result);
    }
}

impl SocketRecvRequest {
    fn new(
        fd: RawFd,
        buf_ptr: *mut u8,
        len: usize,
        flags: i32,
        state: Arc<OpState<usize>>,
    ) -> Self {
        Self {
            fd,
            buf_ptr,
            len,
            flags,
            state,
        }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn buf_ptr(&self) -> *mut libc::c_void {
        self.buf_ptr as *mut libc::c_void
    }

    pub fn flags(&self) -> i32 {
        self.flags
    }

    pub fn finish(self, result: RS<usize>) {
        complete_op(self.state, result);
    }
}

impl SocketSendRequest {
    fn new(fd: RawFd, data: Vec<u8>, flags: i32, state: Arc<OpState<usize>>) -> Self {
        Self {
            fd,
            flags,
            data,
            sent: 0,
            state,
        }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn flags(&self) -> i32 {
        self.flags
    }

    pub fn data_ptr(&self) -> *const libc::c_void {
        unsafe { self.data.as_ptr().add(self.sent) as *const libc::c_void }
    }

    pub fn remaining_len(&self) -> usize {
        self.data.len().saturating_sub(self.sent)
    }

    pub fn advance(&mut self, sent: usize) {
        self.sent += sent;
    }

    pub fn is_complete(&self) -> bool {
        self.sent >= self.data.len()
    }

    pub fn total_len(&self) -> usize {
        self.data.len()
    }

    pub fn finish(self, result: RS<usize>) {
        complete_op(self.state, result);
    }
}

impl SocketSendRefRequest {
    fn new(
        fd: RawFd,
        data_ptr: *const u8,
        len: usize,
        flags: i32,
        state: Arc<OpState<usize>>,
    ) -> Self {
        Self {
            fd,
            flags,
            data_ptr,
            len,
            sent: 0,
            state,
        }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn flags(&self) -> i32 {
        self.flags
    }

    pub fn data_ptr(&self) -> *const libc::c_void {
        unsafe { self.data_ptr.add(self.sent) as *const libc::c_void }
    }

    pub fn remaining_len(&self) -> usize {
        self.len.saturating_sub(self.sent)
    }

    pub fn advance(&mut self, sent: usize) {
        self.sent += sent;
    }

    pub fn is_complete(&self) -> bool {
        self.sent >= self.len
    }

    pub fn total_len(&self) -> usize {
        self.len
    }

    pub fn finish(self, result: RS<usize>) {
        complete_op(self.state, result);
    }
}

impl SocketShutdownRequest {
    fn new(fd: RawFd, how: i32, state: Arc<OpState<()>>) -> Self {
        Self { fd, how, state }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn how(&self) -> i32 {
        self.how
    }

    pub fn finish(self, result: RS<()>) {
        complete_op(self.state, result);
    }
}

impl SocketCloseRequest {
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

enum SocketFutureState<T> {
    Init,
    Pending(Arc<OpState<T>>),
    Done,
}

struct SocketOpenFuture {
    domain: i32,
    socket_type: i32,
    protocol: i32,
    state: SocketFutureState<RawFd>,
}

struct SocketConnectFuture {
    fd: RawFd,
    addr: Option<crate::imp::native::linux::io_uring::iouring::SockAddrBuf>,
    state: SocketFutureState<()>,
}

struct SocketAcceptFuture {
    fd: RawFd,
    state: SocketFutureState<(RawFd, SocketAddr)>,
}

struct SocketRecvIntoFuture<'a> {
    fd: RawFd,
    buf_ptr: *mut u8,
    len: usize,
    flags: i32,
    state: SocketFutureState<usize>,
    _marker: PhantomData<&'a mut [u8]>,
}

unsafe impl<'a> Send for SocketRecvIntoFuture<'a> {}

struct SocketSendFuture {
    fd: RawFd,
    data: Option<Vec<u8>>,
    flags: i32,
    state: SocketFutureState<usize>,
}

struct SocketSendRefFuture<'a> {
    fd: RawFd,
    data_ptr: *const u8,
    len: usize,
    flags: i32,
    state: SocketFutureState<usize>,
    _marker: PhantomData<&'a [u8]>,
}

struct SocketShutdownFuture {
    fd: RawFd,
    how: i32,
    state: SocketFutureState<()>,
}

struct SocketCloseFuture {
    fd: RawFd,
    state: SocketFutureState<()>,
}

impl SocketOpenFuture {
    fn new(domain: i32, socket_type: i32, protocol: i32) -> Self {
        Self {
            domain,
            socket_type,
            protocol,
            state: SocketFutureState::Init,
        }
    }
}

impl SocketConnectFuture {
    fn new(fd: RawFd, addr: SocketAddr) -> Self {
        Self {
            fd,
            addr: Some(socket_addr_to_raw(addr)),
            state: SocketFutureState::Init,
        }
    }
}

impl SocketAcceptFuture {
    fn new(fd: RawFd) -> Self {
        Self {
            fd,
            state: SocketFutureState::Init,
        }
    }
}

impl<'a> SocketRecvIntoFuture<'a> {
    fn new(fd: RawFd, buf: &'a mut [u8], flags: i32) -> Self {
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
    fn new(fd: RawFd, data: Vec<u8>, flags: i32) -> Self {
        Self {
            fd,
            data: Some(data),
            flags,
            state: SocketFutureState::Init,
        }
    }
}

impl<'a> SocketSendRefFuture<'a> {
    fn new(fd: RawFd, data: &'a [u8], flags: i32) -> Self {
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
    fn new(fd: RawFd, how: i32) -> Self {
        Self {
            fd,
            how,
            state: SocketFutureState::Init,
        }
    }
}

impl SocketCloseFuture {
    fn new(fd: RawFd) -> Self {
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
                let addr = self.addr.take().unwrap();
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
                let data = self.data.take().unwrap();
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

fn socket_addr_to_raw(
    addr: SocketAddr,
) -> crate::imp::native::linux::io_uring::iouring::SockAddrBuf {
    crate::io::net::socket_addr_to_storage(addr).expect("socket addr to storage conversion failed")
}

pub fn raw_to_socket_addr(
    addr: &crate::imp::native::linux::io_uring::iouring::SockAddrBuf,
) -> RS<SocketAddr> {
    crate::io::net::sockaddr_to_socket_addr(addr)
}

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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::imp::native::linux::io_uring::worker_ring::{
        set_current_worker_ring, unset_current_worker_ring,
    };
    use crate::task::async_::spawn_task_detached;
    use tokio::task::yield_now;

    fn install_test_ring() -> Arc<WorkerLocalRing> {
        let ring = Arc::new(WorkerLocalRing::new());
        set_current_worker_ring(ring.clone());
        ring
    }

    #[tokio::test(flavor = "current_thread")]
    async fn socket_and_connect_enqueue_requests() {
        let ring = install_test_ring();
        let create_task = spawn_task_detached("test", async {
            socket(libc::AF_INET, libc::SOCK_STREAM, 0).await
        })
        .unwrap();
        yield_now().await;
        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::Socket(SocketIoRequest::Socket(request)) => {
                assert_eq!(request.domain(), libc::AF_INET);
                assert_eq!(request.socket_type(), libc::SOCK_STREAM);
                assert_eq!(request.protocol(), 0);
                request.finish(Ok(41));
            }
            _ => panic!("expected socket request"),
        }
        let sock = create_task.await.unwrap().unwrap().unwrap();
        assert_eq!(sock.fd(), 41);

        let connect_task = spawn_task_detached("test", async move {
            connect(&sock, "127.0.0.1:9527".parse().unwrap()).await
        })
        .unwrap();
        yield_now().await;
        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::Socket(SocketIoRequest::Connect(request)) => {
                assert_eq!(request.fd(), 41);
                request.finish(Ok(()));
            }
            _ => panic!("expected connect request"),
        }
        connect_task.await.unwrap().unwrap().unwrap();
        unset_current_worker_ring();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn accept_recv_send_shutdown_and_close_enqueue_requests() {
        let ring = install_test_ring();
        let sock = IoSocket { fd: 51 };

        let accept_task = spawn_task_detached("test", async move { accept(&sock).await }).unwrap();
        yield_now().await;
        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::Socket(SocketIoRequest::Accept(request)) => {
                assert_eq!(request.fd(), 51);
                request.finish(Ok((61, "127.0.0.1:9010".parse().unwrap())));
            }
            _ => panic!("expected accept request"),
        }
        let (accepted, addr) = accept_task.await.unwrap().unwrap().unwrap();
        assert_eq!(accepted.fd(), 61);
        assert_eq!(addr, "127.0.0.1:9010".parse::<SocketAddr>().unwrap());

        let recv_task = spawn_task_detached("test", async move {
            let mut buf = [0u8; 8];
            let read = recv_into(&accepted, &mut buf, libc::MSG_DONTWAIT).await?;
            Ok::<_, mudu::error::err::MError>((read, buf))
        })
        .unwrap();
        yield_now().await;
        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::Socket(SocketIoRequest::Recv(request)) => {
                assert_eq!(request.fd(), 61);
                assert_eq!(request.len(), 8);
                assert_eq!(request.flags(), libc::MSG_DONTWAIT);
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        [7u8, 8, 9].as_ptr(),
                        request.buf_ptr() as *mut u8,
                        3,
                    );
                }
                request.finish(Ok(3));
            }
            _ => panic!("expected recv request"),
        }
        let (read, recv_buf) = recv_task.await.unwrap().unwrap().unwrap();
        assert_eq!(read, 3);
        assert_eq!(&recv_buf[..3], &[7, 8, 9]);

        let send_sock = IoSocket { fd: 71 };
        let send_task = spawn_task_detached("test", async move {
            send(&send_sock, vec![1, 2, 3], libc::MSG_NOSIGNAL).await
        })
        .unwrap();
        yield_now().await;
        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::Socket(SocketIoRequest::Send(request)) => {
                assert_eq!(request.fd(), 71);
                assert_eq!(request.flags(), libc::MSG_NOSIGNAL);
                assert_eq!(request.remaining_len(), 3);
                request.finish(Ok(3));
            }
            _ => panic!("expected send request"),
        }
        assert_eq!(send_task.await.unwrap().unwrap().unwrap(), 3);

        let shutdown_sock = IoSocket { fd: 71 };
        let shutdown_task = spawn_task_detached("test", async move {
            shutdown(&shutdown_sock, libc::SHUT_WR).await
        })
        .unwrap();
        yield_now().await;
        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::Socket(SocketIoRequest::Shutdown(request)) => {
                assert_eq!(request.fd(), 71);
                assert_eq!(request.how(), libc::SHUT_WR);
                request.finish(Ok(()));
            }
            _ => panic!("expected shutdown request"),
        }
        shutdown_task.await.unwrap().unwrap().unwrap();

        let close_task =
            spawn_task_detached("test", async move { close(IoSocket { fd: 71 }).await }).unwrap();
        yield_now().await;
        match ring.take_pending().unwrap().unwrap().1 {
            WorkerRingOp::Socket(SocketIoRequest::Close(request)) => {
                assert_eq!(request.fd(), 71);
                request.finish(Ok(()));
            }
            _ => panic!("expected close request"),
        }
        close_task.await.unwrap().unwrap().unwrap();
        unset_current_worker_ring();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn socket_without_current_ring_returns_error() {
        unset_current_worker_ring();
        let err = socket(libc::AF_INET, libc::SOCK_STREAM, 0)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), EC::NoSuchElement);
    }
}
