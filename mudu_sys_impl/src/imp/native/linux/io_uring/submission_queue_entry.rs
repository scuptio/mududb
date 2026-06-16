use crate::uring::SockAddrBuf;
use std::ffi::CStr;
use std::marker::PhantomData;
use std::os::fd::RawFd;

pub struct SubmissionQueueEntry<'a> {
    raw: *mut rliburing::io_uring_sqe,
    _marker: PhantomData<&'a mut rliburing::io_uring_sqe>,
}

impl SubmissionQueueEntry<'_> {
    pub fn new(ptr: *mut rliburing::io_uring_sqe) -> Self {
        Self {
            raw: ptr,
            _marker: PhantomData,
        }
    }

    pub fn set_user_data(&mut self, user_data: u64) {
        unsafe {
            (*self.raw).user_data = user_data;
        }
    }

    pub fn prep_openat(&mut self, dirfd: RawFd, path: &CStr, flags: i32, mode: u32) {
        unsafe {
            rliburing::io_uring_prep_openat(self.raw, dirfd, path.as_ptr(), flags, mode);
        }
    }

    pub fn prep_mkdirat(&mut self, dirfd: RawFd, path: &CStr, mode: u32) {
        unsafe {
            rliburing::io_uring_prep_mkdirat(self.raw, dirfd, path.as_ptr(), mode);
        }
    }

    pub fn prep_close(&mut self, fd: RawFd) {
        unsafe { rliburing::io_uring_prep_close(self.raw, fd) };
    }

    pub fn prep_read_raw(&mut self, fd: RawFd, buf: *mut u8, len: usize, offset: u64) {
        unsafe {
            rliburing::io_uring_prep_read(self.raw, fd, buf.cast(), len as _, offset as _);
        }
    }

    pub fn prep_write_raw(&mut self, fd: RawFd, buf: *const u8, len: usize, offset: u64) {
        unsafe {
            rliburing::io_uring_prep_write(self.raw, fd, buf.cast(), len as _, offset as _);
        }
    }

    pub fn prep_fsync(&mut self, fd: RawFd) {
        unsafe { rliburing::io_uring_prep_fsync(self.raw, fd, 0) };
    }

    pub fn prep_socket(&mut self, domain: i32, socket_type: i32, protocol: i32, flags: u32) {
        unsafe { rliburing::io_uring_prep_socket(self.raw, domain, socket_type, protocol, flags) };
    }

    pub fn prep_connect(&mut self, fd: RawFd, addr: &SockAddrBuf) {
        unsafe {
            rliburing::io_uring_prep_connect(self.raw, fd, addr.sockaddr_ptr(), addr.socklen())
        };
    }

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

    pub fn prep_recv_raw(&mut self, fd: RawFd, buf: *mut u8, len: usize, flags: i32) {
        unsafe { rliburing::io_uring_prep_recv(self.raw, fd, buf.cast(), len as _, flags) };
    }

    pub fn prep_send_raw(&mut self, fd: RawFd, buf: *const u8, len: usize, flags: i32) {
        unsafe { rliburing::io_uring_prep_send(self.raw, fd, buf.cast(), len as _, flags) };
    }

    pub fn prep_shutdown(&mut self, fd: RawFd, how: i32) {
        unsafe { rliburing::io_uring_prep_shutdown(self.raw, fd, how) };
    }

    pub fn prep_unlinkat(&mut self, dirfd: RawFd, path: &CStr, flags: i32) {
        unsafe {
            rliburing::io_uring_prep_unlinkat(self.raw, dirfd, path.as_ptr(), flags);
        }
    }

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
