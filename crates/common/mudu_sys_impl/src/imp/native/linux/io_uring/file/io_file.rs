use super::*;

/// An open file descriptor used with the io_uring worker ring.
pub struct IoFile {
    pub(crate) fd: RawFd,
}

impl IoFile {
    /// Returns `true` if the descriptor is the invalid sentinel value 0.
    pub fn is_invalid(&self) -> bool {
        self.fd == 0
    }

    /// Wraps a raw file descriptor.
    pub fn new(fd: RawFd) -> Self {
        Self { fd }
    }

    /// Returns the raw file descriptor.
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Wraps a raw file descriptor.
    pub fn from_raw_fd(fd: RawFd) -> Self {
        Self::new(fd)
    }
}
