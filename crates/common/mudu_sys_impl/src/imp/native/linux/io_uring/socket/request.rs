use std::net::SocketAddr;
use std::os::fd::RawFd;
use std::sync::Arc;

use mudu::common::result::RS;

use crate::imp::io::user_io::{complete_op, OpState};

/// A socket operation queued for submission to the worker ring.
pub enum SocketIoRequest {
    /// Create a socket.
    Socket(SocketOpenRequest),
    /// Connect a socket.
    Connect(SocketConnectRequest),
    /// Accept a connection.
    Accept(SocketAcceptRequest),
    /// Receive into a buffer.
    Recv(SocketRecvRequest),
    /// Send an owned buffer.
    Send(SocketSendRequest),
    /// Send from a borrowed buffer.
    SendRef(SocketSendRefRequest),
    /// Shutdown one or both directions.
    Shutdown(SocketShutdownRequest),
    /// Close the socket.
    Close(SocketCloseRequest),
}

/// A socket operation submitted to the ring and not yet completed; keeps
/// request-owned buffers alive until the CQE arrives.
pub enum SocketInflightOp {
    /// In-flight socket creation.
    Open(Box<SocketOpenRequest>),
    /// In-flight connect.
    Connect(Box<SocketConnectRequest>),
    /// In-flight accept.
    Accept(Box<SocketAcceptRequest>),
    /// In-flight recv.
    Recv(Box<SocketRecvRequest>),
    /// In-flight send of an owned buffer.
    Send(Box<SocketSendRequest>),
    /// In-flight send of a borrowed buffer.
    SendRef(Box<SocketSendRefRequest>),
    /// In-flight shutdown.
    Shutdown(Box<SocketShutdownRequest>),
    /// In-flight close.
    Close(Box<SocketCloseRequest>),
}

/// Request to create a socket via the ring.
pub struct SocketOpenRequest {
    domain: i32,
    socket_type: i32,
    protocol: i32,
    state: Arc<OpState<RawFd>>,
}

/// Request to connect a socket via the ring.
pub struct SocketConnectRequest {
    fd: RawFd,
    addr: crate::imp::native::linux::io_uring::iouring::SockAddrBuf,
    state: Arc<OpState<()>>,
}

/// Request to accept a connection via the ring.
pub struct SocketAcceptRequest {
    fd: RawFd,
    addr: crate::imp::native::linux::io_uring::iouring::SockAddrBuf,
    state: Arc<OpState<(RawFd, SocketAddr)>>,
}

/// Request to receive from a socket into a raw buffer via the ring.
pub struct SocketRecvRequest {
    fd: RawFd,
    buf_ptr: *mut u8,
    len: usize,
    flags: i32,
    state: Arc<OpState<usize>>,
}

/// Request to send an owned buffer via the ring; resubmitted on short
/// writes until the whole buffer is sent.
pub struct SocketSendRequest {
    fd: RawFd,
    flags: i32,
    data: Vec<u8>,
    sent: usize,
    state: Arc<OpState<usize>>,
}

/// Request to send from a borrowed buffer via the ring; resubmitted on
/// short writes until the whole buffer is sent.
pub struct SocketSendRefRequest {
    fd: RawFd,
    flags: i32,
    data_ptr: *const u8,
    len: usize,
    sent: usize,
    state: Arc<OpState<usize>>,
}

/// Request to shut down one or both directions of a socket via the ring.
pub struct SocketShutdownRequest {
    fd: RawFd,
    how: i32,
    state: Arc<OpState<()>>,
}

/// Request to close a socket via the ring.
pub struct SocketCloseRequest {
    fd: RawFd,
    state: Arc<OpState<()>>,
}

impl SocketOpenRequest {
    pub(super) fn new(
        domain: i32,
        socket_type: i32,
        protocol: i32,
        state: Arc<OpState<RawFd>>,
    ) -> Self {
        Self {
            domain,
            socket_type,
            protocol,
            state,
        }
    }

    /// Returns the address family (e.g. `AF_INET`).
    pub fn domain(&self) -> i32 {
        self.domain
    }

    /// Returns the socket type (e.g. `SOCK_STREAM`).
    pub fn socket_type(&self) -> i32 {
        self.socket_type
    }

    /// Returns the protocol (e.g. 0 for the default).
    pub fn protocol(&self) -> i32 {
        self.protocol
    }

    /// Delivers the completion result (the new socket fd) to the waiting
    /// task.
    pub fn finish(self, result: RS<RawFd>) {
        complete_op(self.state, result);
    }
}

impl SocketConnectRequest {
    pub(super) fn new(
        fd: RawFd,
        addr: crate::imp::native::linux::io_uring::iouring::SockAddrBuf,
        state: Arc<OpState<()>>,
    ) -> Self {
        Self { fd, addr, state }
    }

    /// Returns the socket file descriptor being connected.
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Returns the target address.
    pub fn addr(&self) -> &crate::imp::native::linux::io_uring::iouring::SockAddrBuf {
        &self.addr
    }

    /// Delivers the completion result to the waiting task.
    pub fn finish(self, result: RS<()>) {
        complete_op(self.state, result);
    }
}

impl SocketAcceptRequest {
    pub(super) fn new(fd: RawFd, state: Arc<OpState<(RawFd, SocketAddr)>>) -> Self {
        Self {
            fd,
            addr: crate::imp::native::linux::io_uring::iouring::SockAddrBuf::new_empty(),
            state,
        }
    }

    /// Returns the listening socket file descriptor.
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Returns the buffer the kernel writes the peer address into.
    pub fn addr_mut(&mut self) -> &mut crate::imp::native::linux::io_uring::iouring::SockAddrBuf {
        &mut self.addr
    }

    /// Returns the buffer holding the peer address after completion.
    pub fn addr(&self) -> &crate::imp::native::linux::io_uring::iouring::SockAddrBuf {
        &self.addr
    }

    /// Delivers the completion result (new connection fd and peer address)
    /// to the waiting task.
    pub fn finish(self, result: RS<(RawFd, SocketAddr)>) {
        complete_op(self.state, result);
    }
}

impl SocketRecvRequest {
    pub(super) fn new(
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

    /// Returns the socket file descriptor to receive from.
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Returns the number of bytes to receive.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if no bytes are requested.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the buffer the kernel writes into; must stay valid until the
    /// completion is processed.
    pub fn buf_ptr(&self) -> *mut libc::c_void {
        self.buf_ptr as *mut libc::c_void
    }

    /// Returns the recv flags.
    pub fn flags(&self) -> i32 {
        self.flags
    }

    /// Delivers the number of bytes received to the waiting task.
    pub fn finish(self, result: RS<usize>) {
        complete_op(self.state, result);
    }
}

impl SocketSendRequest {
    pub(super) fn new(fd: RawFd, data: Vec<u8>, flags: i32, state: Arc<OpState<usize>>) -> Self {
        Self {
            fd,
            flags,
            data,
            sent: 0,
            state,
        }
    }

    /// Returns the socket file descriptor to send on.
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Returns the send flags.
    pub fn flags(&self) -> i32 {
        self.flags
    }

    /// Returns a pointer to the first not-yet-sent byte; must stay valid
    /// until the completion is processed.
    pub fn data_ptr(&self) -> *const libc::c_void {
        unsafe { self.data.as_ptr().add(self.sent) as *const libc::c_void }
    }

    /// Returns the number of bytes not yet sent.
    pub fn remaining_len(&self) -> usize {
        self.data.len().saturating_sub(self.sent)
    }

    /// Advances the send cursor after `sent` bytes completed.
    pub fn advance(&mut self, sent: usize) {
        self.sent += sent;
    }

    /// Returns `true` once the whole buffer has been sent.
    pub fn is_complete(&self) -> bool {
        self.sent >= self.data.len()
    }

    /// Returns the total number of bytes to send.
    pub fn total_len(&self) -> usize {
        self.data.len()
    }

    /// Delivers the sent byte count to the waiting task.
    pub fn finish(self, result: RS<usize>) {
        complete_op(self.state, result);
    }
}

impl SocketSendRefRequest {
    pub(super) fn new(
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

    /// Returns the socket file descriptor to send on.
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Returns the send flags.
    pub fn flags(&self) -> i32 {
        self.flags
    }

    /// Returns a pointer to the first not-yet-sent byte; must stay valid
    /// until the completion is processed.
    pub fn data_ptr(&self) -> *const libc::c_void {
        unsafe { self.data_ptr.add(self.sent) as *const libc::c_void }
    }

    /// Returns the number of bytes not yet sent.
    pub fn remaining_len(&self) -> usize {
        self.len.saturating_sub(self.sent)
    }

    /// Advances the send cursor after `sent` bytes completed.
    pub fn advance(&mut self, sent: usize) {
        self.sent += sent;
    }

    /// Returns `true` once the whole buffer has been sent.
    pub fn is_complete(&self) -> bool {
        self.sent >= self.len
    }

    /// Returns the total number of bytes to send.
    pub fn total_len(&self) -> usize {
        self.len
    }

    /// Delivers the sent byte count to the waiting task.
    pub fn finish(self, result: RS<usize>) {
        complete_op(self.state, result);
    }
}

impl SocketShutdownRequest {
    pub(super) fn new(fd: RawFd, how: i32, state: Arc<OpState<()>>) -> Self {
        Self { fd, how, state }
    }

    /// Returns the socket file descriptor to shut down.
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Returns the shutdown mode (`SHUT_RD`/`SHUT_WR`/`SHUT_RDWR`).
    pub fn how(&self) -> i32 {
        self.how
    }

    /// Delivers the completion result to the waiting task.
    pub fn finish(self, result: RS<()>) {
        complete_op(self.state, result);
    }
}

impl SocketCloseRequest {
    pub(super) fn new(fd: RawFd, state: Arc<OpState<()>>) -> Self {
        Self { fd, state }
    }

    /// Returns the socket file descriptor to close.
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Delivers the completion result to the waiting task.
    pub fn finish(self, result: RS<()>) {
        complete_op(self.state, result);
    }
}
