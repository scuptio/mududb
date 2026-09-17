#[cfg(unix)]
pub type RawFd = std::os::fd::RawFd;

#[cfg(not(unix))]
pub type RawFd = i32;
