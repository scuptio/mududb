#[cfg(target_os = "linux")]
mod linux {
    use std::time::Duration;

    /// `IORING_REGISTER_IOWQ_AFF` from linux/io_uring.h (not exposed by
    /// the system liburing headers, so we register by raw opcode).
    const IORING_REGISTER_IOWQ_AFF: u32 = 17;
    /// `IORING_REGISTER_IOWQ_MAX_WORKERS` from linux/io_uring.h.
    const IORING_REGISTER_IOWQ_MAX_WORKERS: u32 = 19;

    /// Raw io_uring_register(2) via syscall: the libc crate in this
    /// workspace does not export `io_uring_register`.
    unsafe fn ring_register(fd: i32, opcode: u32, arg: *const libc::c_void, nr_args: u32) -> i32 {
        libc::syscall(libc::SYS_io_uring_register, fd, opcode, arg, nr_args) as i32
    }

    pub struct IoUring {
        raw: rliburing::io_uring,
        exited: bool,
    }

    #[derive(Clone, Copy)]
    pub struct SockAddrBuf {
        raw: rliburing::sockaddr_storage,
        len: u32,
    }

    impl IoUring {
        pub fn new(_entries: u32) -> Result<Self, i32> {
            // Miri does not support the io_uring syscalls/FFI used by this
            // backend (e.g. `io_uring_queue_init_params`). Report the ring as
            // unavailable so callers can skip tests gracefully instead of
            // hitting an unsupported-operation error.
            #[cfg(miri)]
            {
                return Err(-libc::ENOSYS);
            }
            #[cfg(not(miri))]
            {
                let mut raw = unsafe { std::mem::zeroed() };
                let mut param = unsafe { std::mem::zeroed() };
                let rc = unsafe {
                    rliburing::io_uring_queue_init_params(_entries, &mut raw, &mut param)
                };
                if rc != 0 {
                    return Err(rc);
                }
                Ok(Self { raw, exited: false })
            }
        }

        pub fn next_sqe(&mut self) -> Option<SubmissionQueueEntry<'_>> {
            let sqe = unsafe { rliburing::io_uring_get_sqe(&mut self.raw) };
            (!sqe.is_null()).then_some(SubmissionQueueEntry::new(sqe))
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

        /// Quick functional probe: create a ring, submit a NOP, and wait for the
        /// completion. Returns `false` if any step fails or the wait times out.
        pub fn probe() -> bool {
            let mut ring = match Self::new(8) {
                Ok(ring) => ring,
                Err(_) => return false,
            };
            let mut sqe = match ring.next_sqe() {
                Some(sqe) => sqe,
                None => return false,
            };
            sqe.set_user_data(0);
            sqe.prep_nop();
            if ring.submit() < 0 {
                return false;
            }
            match ring.wait_timeout(Duration::from_millis(200)) {
                Ok(c) => c.result() >= 0,
                Err(_) => false,
            }
        }

        /// Pins this ring's io_wq worker threads to a single CPU, so
        /// buffered-write SQEs are executed on the worker's own core
        /// instead of being scheduled onto arbitrary (possibly busy)
        /// cores. Returns the raw io_uring_register result (0 = ok).
        pub fn register_iowq_affinity(&mut self, cpu: usize) -> i32 {
            unsafe {
                let mut set: libc::cpu_set_t = std::mem::zeroed();
                libc::CPU_ZERO(&mut set);
                libc::CPU_SET(cpu, &mut set);
                ring_register(
                    self.raw.ring_fd,
                    IORING_REGISTER_IOWQ_AFF,
                    &set as *const libc::cpu_set_t as *const libc::c_void,
                    std::mem::size_of::<libc::cpu_set_t>() as u32,
                )
            }
        }

        /// Raises this ring's io_wq worker caps (bounded, unbounded),
        /// so concurrent buffered writes never queue waiting for a
        /// worker. Returns the raw io_uring_register result (0 = ok).
        pub fn register_iowq_max_workers(&mut self, bounded: u32, unbounded: u32) -> i32 {
            let bounds = [bounded, unbounded];
            unsafe {
                ring_register(
                    self.raw.ring_fd,
                    IORING_REGISTER_IOWQ_MAX_WORKERS,
                    bounds.as_ptr() as *const libc::c_void,
                    2,
                )
            }
        }

        pub fn exit(&mut self) {
            if self.exited {
                return;
            }
            unsafe { rliburing::io_uring_queue_exit(&mut self.raw) };
            self.exited = true;
        }

        fn take_completion(&mut self, cqe_ptr: *mut rliburing::io_uring_cqe) -> Completion {
            let completion =
                Completion::new(unsafe { (*cqe_ptr).user_data }, unsafe { (*cqe_ptr).res });
            unsafe { rliburing::io_uring_cqe_seen(&mut self.raw, cqe_ptr) };
            completion
        }
    }

    impl Drop for IoUring {
        fn drop(&mut self) {
            self.exit();
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

        pub fn is_empty(&self) -> bool {
            self.len == 0
        }

        pub(crate) fn from_raw(raw: rliburing::sockaddr_storage, len: u32) -> Self {
            Self { raw, len }
        }

        pub(crate) fn raw(&self) -> &rliburing::sockaddr_storage {
            &self.raw
        }

        pub(crate) fn sockaddr_ptr(&self) -> *const rliburing::sockaddr {
            (&self.raw as *const rliburing::sockaddr_storage).cast()
        }

        pub(crate) fn sockaddr_mut_ptr(&mut self) -> *mut rliburing::sockaddr {
            (&mut self.raw as *mut rliburing::sockaddr_storage).cast()
        }

        pub(crate) fn socklen(&self) -> rliburing::socklen_t {
            self.len
        }

        pub(crate) fn socklen_mut_ptr(&mut self) -> *mut rliburing::socklen_t {
            &mut self.len
        }
    }

    pub use crate::imp::native::linux::io_uring::completion::Completion as Cqe;
    use crate::imp::native::linux::io_uring::completion::Completion;
    pub use crate::imp::native::linux::io_uring::submission_queue_entry::SubmissionQueueEntry as Sqe;
    use crate::imp::native::linux::io_uring::submission_queue_entry::SubmissionQueueEntry;
    pub use IoUring as Ring;
    pub use SockAddrBuf as SocketAddrBuf;

    #[cfg(test)]
    mod tests {
        #![allow(clippy::unwrap_used)]

        /// Regression test for the io_uring functional probe.
        ///
        /// The probe must return quickly and must not hang, even on systems where
        /// io_uring syscalls are blocked by seccomp.
        #[test]
        fn probe_returns_without_hanging() {
            let start = std::time::Instant::now();
            let available = super::IoUring::probe();
            let elapsed = start.elapsed();
            assert!(
                elapsed < std::time::Duration::from_secs(2),
                "io_uring probe should not hang, took {:?}",
                elapsed
            );
            // On Linux hosts with a working io_uring this is true; in restricted
            // containers it is false. Either is acceptable as long as it returns.
            let _ = available;
        }
    }
}

#[cfg(target_os = "linux")]
pub use crate::imp::native::linux::io_uring::submission_queue_entry::SubmissionQueueEntry;
#[cfg(target_os = "linux")]
pub use linux::{Cqe, IoUring, Ring, SockAddrBuf, SocketAddrBuf, Sqe};
