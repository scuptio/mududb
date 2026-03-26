use std::net::SocketAddr;
use std::os::fd::RawFd;

pub(in crate::server_ur) struct WorkerConnection {
    fd: RawFd,
    remote_addr: SocketAddr,
    read_buf: Vec<u8>,
    recv_buf: Box<[u8; 8192]>,
    pending_write: Vec<u8>,
    send_inflight: Option<Vec<u8>>,
    recv_inflight: bool,
    recv_ready_queued: bool,
    send_ready_queued: bool,
    close_submitted: bool,
}

impl WorkerConnection {
    pub(in crate::server_ur) fn new(fd: RawFd, remote_addr: SocketAddr) -> Self {
        Self {
            fd,
            remote_addr,
            read_buf: Vec::with_capacity(4096),
            recv_buf: Box::new([0u8; 8192]),
            pending_write: Vec::with_capacity(4096),
            send_inflight: None,
            recv_inflight: false,
            recv_ready_queued: false,
            send_ready_queued: false,
            close_submitted: false,
        }
    }

    pub(in crate::server_ur) fn fd(&self) -> RawFd {
        self.fd
    }

    pub(in crate::server_ur) fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    pub(in crate::server_ur) fn read_buf(&self) -> &[u8] {
        &self.read_buf
    }

    pub(in crate::server_ur) fn read_buf_mut(&mut self) -> &mut Vec<u8> {
        &mut self.read_buf
    }

    pub(in crate::server_ur) fn recv_buf_mut_ptr(&mut self) -> *mut u8 {
        self.recv_buf.as_mut_ptr()
    }

    pub(in crate::server_ur) fn recv_buf_len(&self) -> usize {
        self.recv_buf.len()
    }

    pub(in crate::server_ur) fn recv_slice(&self, len: usize) -> &[u8] {
        &self.recv_buf[..len]
    }

    pub(in crate::server_ur) fn pending_write(&self) -> &[u8] {
        &self.pending_write
    }

    pub(in crate::server_ur) fn pending_write_mut(&mut self) -> &mut Vec<u8> {
        &mut self.pending_write
    }

    pub(in crate::server_ur) fn extend_pending_write(&mut self, payload: &[u8]) {
        self.pending_write.extend_from_slice(payload);
    }

    pub(in crate::server_ur) fn take_pending_write(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending_write)
    }

    pub(in crate::server_ur) fn send_inflight(&self) -> Option<&Vec<u8>> {
        self.send_inflight.as_ref()
    }

    pub(in crate::server_ur) fn set_send_inflight(&mut self, payload: Option<Vec<u8>>) {
        self.send_inflight = payload;
    }

    pub(in crate::server_ur) fn take_send_inflight(&mut self) -> Option<Vec<u8>> {
        self.send_inflight.take()
    }

    pub(in crate::server_ur) fn recv_inflight(&self) -> bool {
        self.recv_inflight
    }

    pub(in crate::server_ur) fn set_recv_inflight(&mut self, value: bool) {
        self.recv_inflight = value;
    }

    pub(in crate::server_ur) fn recv_ready_queued(&self) -> bool {
        self.recv_ready_queued
    }

    pub(in crate::server_ur) fn set_recv_ready_queued(&mut self, value: bool) {
        self.recv_ready_queued = value;
    }

    pub(in crate::server_ur) fn send_ready_queued(&self) -> bool {
        self.send_ready_queued
    }

    pub(in crate::server_ur) fn set_send_ready_queued(&mut self, value: bool) {
        self.send_ready_queued = value;
    }

    pub(in crate::server_ur) fn close_submitted(&self) -> bool {
        self.close_submitted
    }

    pub(in crate::server_ur) fn set_close_submitted(&mut self, value: bool) {
        self.close_submitted = value;
    }
}
