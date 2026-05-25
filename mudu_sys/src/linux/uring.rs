#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::CStr;
    use std::marker::PhantomData;
    use std::os::fd::RawFd;
    use std::time::Duration;

    pub struct IoUring {
        raw: rliburing::io_uring,
        exited: bool,
    }

    pub struct SubmissionQueueEntry<'a> {
        raw: *mut rliburing::io_uring_sqe,
        _marker: PhantomData<&'a mut rliburing::io_uring_sqe>,
    }

    #[derive(Clone, Copy, Debug)]
    pub struct Completion {
        user_data: u64,
        result: i32,
    }

    #[derive(Clone, Copy)]
    pub struct SockAddrBuf {
        raw: rliburing::sockaddr_storage,
        len: u32,
    }

    impl IoUring {
        pub fn new(entries: u32) -> Result<Self, i32> {
            let mut raw = unsafe { std::mem::zeroed() };
            let mut param = unsafe { std::mem::zeroed() };
            let rc =
                unsafe { rliburing::io_uring_queue_init_params(entries, &mut raw, &mut param) };
            if rc != 0 {
                return Err(rc);
            }
            Ok(Self { raw, exited: false })
        }

        pub fn next_sqe(&mut self) -> Option<SubmissionQueueEntry<'_>> {
            let sqe = unsafe { rliburing::io_uring_get_sqe(&mut self.raw) };
            (!sqe.is_null()).then_some(SubmissionQueueEntry {
                raw: sqe,
                _marker: PhantomData,
            })
        }

        pub fn submit(&mut self) -> i32 {
            unsafe { rliburing::io_uring_submit(&mut self.raw) }
        }

        pub fn wait(&mut self) -> Result<Completion, i32> {
            let mut cqe_ptr: *mut rliburing::io_uring_cqe = std::ptr::null_mut();
            let rc = unsafe { rliburing::io_uring_wait_cqe(&mut self.raw, &mut cqe_ptr) };
            if rc < 0 {
                return Err(rc);
            }
            Ok(self.take_completion(cqe_ptr))
        }

        pub fn wait_timeout(&mut self, timeout: Duration) -> Result<Completion, i32> {
            let mut cqe_ptr: *mut rliburing::io_uring_cqe = std::ptr::null_mut();
            let mut ts = rliburing::__kernel_timespec {
                tv_sec: timeout.as_secs() as i64,
                tv_nsec: timeout.subsec_nanos() as i64,
            };
            let rc = unsafe {
                rliburing::io_uring_wait_cqe_timeout(&mut self.raw, &mut cqe_ptr, &mut ts)
            };
            if rc < 0 {
                return Err(rc);
            }
            Ok(self.take_completion(cqe_ptr))
        }

        pub fn peek(&mut self) -> Result<Option<Completion>, i32> {
            let mut cqe_ptr: *mut rliburing::io_uring_cqe = std::ptr::null_mut();
            let rc = unsafe { rliburing::io_uring_peek_cqe(&mut self.raw, &mut cqe_ptr) };
            if rc == -libc::EAGAIN || cqe_ptr.is_null() {
                return Ok(None);
            }
            if rc < 0 {
                return Err(rc);
            }
            Ok(Some(self.take_completion(cqe_ptr)))
        }

        pub fn exit(&mut self) {
            if self.exited {
                return;
            }
            unsafe { rliburing::io_uring_queue_exit(&mut self.raw) };
            self.exited = true;
        }

        fn take_completion(&mut self, cqe_ptr: *mut rliburing::io_uring_cqe) -> Completion {
            let completion = Completion {
                user_data: unsafe { (*cqe_ptr).user_data },
                result: unsafe { (*cqe_ptr).res },
            };
            unsafe { rliburing::io_uring_cqe_seen(&mut self.raw, cqe_ptr) };
            completion
        }
    }

    impl Drop for IoUring {
        fn drop(&mut self) {
            self.exit();
        }
    }

    impl Completion {
        pub fn user_data(&self) -> u64 {
            self.user_data
        }

        pub fn result(&self) -> i32 {
            self.result
        }
    }

    impl SubmissionQueueEntry<'_> {
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
            unsafe {
                rliburing::io_uring_prep_socket(self.raw, domain, socket_type, protocol, flags)
            };
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

    impl SockAddrBuf {
        pub fn new_empty() -> Self {
            Self {
                raw: unsafe { std::mem::zeroed() },
                len: std::mem::size_of::<rliburing::sockaddr_storage>() as u32,
            }
        }

        pub fn len(&self) -> usize {
            self.len as usize
        }

        pub(crate) fn from_raw(raw: rliburing::sockaddr_storage, len: u32) -> Self {
            Self { raw, len }
        }

        pub(crate) fn raw(&self) -> &rliburing::sockaddr_storage {
            &self.raw
        }

        fn sockaddr_ptr(&self) -> *const rliburing::sockaddr {
            (&self.raw as *const rliburing::sockaddr_storage).cast()
        }

        fn sockaddr_mut_ptr(&mut self) -> *mut rliburing::sockaddr {
            (&mut self.raw as *mut rliburing::sockaddr_storage).cast()
        }

        fn socklen(&self) -> rliburing::socklen_t {
            self.len
        }

        fn socklen_mut_ptr(&mut self) -> *mut rliburing::socklen_t {
            &mut self.len
        }
    }

    pub use Completion as Cqe;
    pub use IoUring as Ring;
    pub use SockAddrBuf as SocketAddrBuf;
    pub use SubmissionQueueEntry as Sqe;
}

#[cfg(target_os = "linux")]
pub use linux::{Cqe, IoUring, Ring, SockAddrBuf, SocketAddrBuf, Sqe, SubmissionQueueEntry};
