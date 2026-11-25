#[cfg(target_os = "linux")]
use mudu::common::result::RS;
#[cfg(target_os = "linux")]
use mudu::error::ErrorCode;
#[cfg(target_os = "linux")]
use mudu::mudu_error;

/// OS-specific subsystem - native implementation.
pub struct SysOs;

impl Default for SysOs {
    fn default() -> Self {
        Self::new()
    }
}

impl SysOs {
    /// Creates the OS subsystem handle.
    pub fn new() -> Self {
        Self
    }

    /// Creates a new io_uring instance with a submission queue of `entries`
    /// entries (Linux only).
    #[cfg(target_os = "linux")]
    pub fn io_uring(
        &self,
        entries: u32,
    ) -> RS<crate::imp::native::linux::io_uring::iouring::IoUring> {
        crate::imp::native::linux::io_uring::iouring::IoUring::new(entries)
            .map_err(|e| mudu_error!(ErrorCode::Io, format!("create io_uring error: {e}")))
    }
}

#[cfg(all(test, target_os = "linux"))]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn sys_os_creates_io_uring_instances() {
        // io_uring is blocked on some containerized runners (seccomp denies
        // `io_uring_setup` with EPERM); skip instead of failing, matching the
        // other io_uring tests.
        if !crate::io_uring_available() {
            eprintln!("skip io_uring test: io_uring unavailable");
            return;
        }
        let os = SysOs::new();
        let _default = SysOs;
        let _ring = os.io_uring(8).unwrap();
    }
    #[test]
    #[allow(
        clippy::default_constructed_unit_structs,
        reason = "verify default OS facade error propagation"
    )]
    fn invalid_ring_size_returns_io_error() {
        let os = SysOs::default();
        let result = os.io_uring(u32::MAX);
        match result {
            Err(error) => assert_eq!(error.ec(), ErrorCode::Io),
            Ok(_) => panic!("invalid submission queue size unexpectedly accepted"),
        }
    }
}
