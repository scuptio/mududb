use crate::imp::native::linux::io_uring::iouring::SockAddrBuf;
use std::ffi::CStr;
use std::marker::PhantomData;
use std::os::fd::RawFd;

/// Borrowed handle to one io_uring submission queue entry; the lifetime is
/// tied to the ring it was obtained from.
///
/// The `prep_*` methods wrap the corresponding `io_uring_prep_*` helpers and
/// only fill in the entry; nothing reaches the kernel until the ring is
/// submitted.
pub struct SubmissionQueueEntry<'a> {
    raw: *mut rliburing::io_uring_sqe,
    _marker: PhantomData<&'a mut rliburing::io_uring_sqe>,
}

impl SubmissionQueueEntry<'_> {
    /// Wraps a raw SQE pointer obtained from `io_uring_get_sqe`.
    pub fn new(ptr: *mut rliburing::io_uring_sqe) -> Self {
        Self {
            raw: ptr,
            _marker: PhantomData,
        }
    }

    /// Sets the `user_data` tag echoed back in the completion.
    pub fn set_user_data(&mut self, user_data: u64) {
        unsafe {
            (*self.raw).user_data = user_data;
        }
    }

    /// Prepares an openat operation.
    pub fn prep_openat(&mut self, dirfd: RawFd, path: &CStr, flags: i32, mode: u32) {
        unsafe {
            rliburing::io_uring_prep_openat(self.raw, dirfd, path.as_ptr(), flags, mode);
        }
    }

    /// Prepares a mkdirat operation.
    pub fn prep_mkdirat(&mut self, dirfd: RawFd, path: &CStr, mode: u32) {
        unsafe {
            rliburing::io_uring_prep_mkdirat(self.raw, dirfd, path.as_ptr(), mode);
        }
    }

    /// Prepares a close operation on `fd`.
    pub fn prep_close(&mut self, fd: RawFd) {
        unsafe { rliburing::io_uring_prep_close(self.raw, fd) };
    }

    /// Prepares a positioned read of `len` bytes from `fd` at `offset` into
    /// `buf`; `buf` must stay valid until the completion is processed.
    pub fn prep_read_raw(&mut self, fd: RawFd, buf: *mut u8, len: usize, offset: u64) {
        unsafe {
            rliburing::io_uring_prep_read(self.raw, fd, buf.cast(), len as _, offset as _);
        }
    }

    /// Prepares a positioned write of `len` bytes from `buf` to `fd` at
    /// `offset`; `buf` must stay valid until the completion is processed.
    pub fn prep_write_raw(&mut self, fd: RawFd, buf: *const u8, len: usize, offset: u64) {
        unsafe {
            rliburing::io_uring_prep_write(self.raw, fd, buf.cast(), len as _, offset as _);
        }
    }

    /// Prepares an fsync of `fd`.
    pub fn prep_fsync(&mut self, fd: RawFd) {
        unsafe { rliburing::io_uring_prep_fsync(self.raw, fd, 0) };
    }

    /// Prepares a socket(2) creation.
    pub fn prep_socket(&mut self, domain: i32, socket_type: i32, protocol: i32, flags: u32) {
        unsafe { rliburing::io_uring_prep_socket(self.raw, domain, socket_type, protocol, flags) };
    }

    /// Prepares a connect of `fd` to `addr`.
    pub fn prep_connect(&mut self, fd: RawFd, addr: &SockAddrBuf) {
        unsafe {
            rliburing::io_uring_prep_connect(self.raw, fd, addr.sockaddr_ptr(), addr.socklen())
        };
    }

    /// Prepares an accept on listening socket `fd`, writing the peer
    /// address into `addr`.
    pub fn prep_accept(&mut self, fd: RawFd, addr: &mut SockAddrBuf, flags: i32) {
        unsafe {
            rliburing::io_uring_prep_accept(
                self.raw,
                fd,
                addr.sockaddr_mut_ptr(),
                addr.socklen_mut_ptr(),
                flags,
            )
        };
    }

    /// Prepares a recv of `len` bytes from `fd` into `buf`; `buf` must stay
    /// valid until the completion is processed.
    pub fn prep_recv_raw(&mut self, fd: RawFd, buf: *mut u8, len: usize, flags: i32) {
        unsafe { rliburing::io_uring_prep_recv(self.raw, fd, buf.cast(), len as _, flags) };
    }

    /// Prepares a send of `len` bytes from `buf` to `fd`; `buf` must stay
    /// valid until the completion is processed.
    pub fn prep_send_raw(&mut self, fd: RawFd, buf: *const u8, len: usize, flags: i32) {
        unsafe { rliburing::io_uring_prep_send(self.raw, fd, buf.cast(), len as _, flags) };
    }

    /// Prepares a shutdown of `fd` (`SHUT_RD`/`SHUT_WR`/`SHUT_RDWR`).
    pub fn prep_shutdown(&mut self, fd: RawFd, how: i32) {
        unsafe { rliburing::io_uring_prep_shutdown(self.raw, fd, how) };
    }

    /// Prepares a no-op operation.
    pub fn prep_nop(&mut self) {
        unsafe { rliburing::io_uring_prep_nop(self.raw) };
    }

    /// Prepares an unlinkat operation.
    pub fn prep_unlinkat(&mut self, dirfd: RawFd, path: &CStr, flags: i32) {
        unsafe {
            rliburing::io_uring_prep_unlinkat(self.raw, dirfd, path.as_ptr(), flags);
        }
    }

    /// Prepares a statx query writing into `statxbuf`; the buffer must stay
    /// valid until the completion is processed.
    pub fn prep_statx(
        &mut self,
        dirfd: RawFd,
        path: &CStr,
        flags: i32,
        mask: u32,
        statxbuf: *mut libc::statx,
    ) {
        unsafe {
            rliburing::io_uring_prep_statx(
                self.raw,
                dirfd,
                path.as_ptr(),
                flags,
                mask,
                statxbuf as *mut rliburing::statx,
            );
        }
    }
}
