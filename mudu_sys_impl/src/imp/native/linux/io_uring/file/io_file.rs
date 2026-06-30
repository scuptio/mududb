use super::*;

pub struct IoFile {
    pub(crate) fd: RawFd,
}

impl IoFile {
    pub fn is_invalid(&self) -> bool {
        self.fd == 0
    }

    pub fn new(fd: RawFd) -> Self {
        Self { fd }
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    pub fn from_raw_fd(fd: RawFd) -> Self {
        Self::new(fd)
    }
}
