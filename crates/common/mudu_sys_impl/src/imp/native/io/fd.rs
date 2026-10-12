/// Raw file descriptor (`std::os::fd::RawFd` on Unix, `i32` elsewhere).
#[cfg(unix)]
pub type RawFd = std::os::fd::RawFd;

/// Raw file descriptor (`std::os::fd::RawFd` on Unix, `i32` elsewhere).
#[cfg(not(unix))]
pub type RawFd = i32;
