use mudu::common::result::RS;
use mudu::error::others::network_error_with_message;
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::fd::{AsRawFd, RawFd};

/// Blocking TCP stream over [`std::net::TcpStream`].
pub struct TcpStream {
    inner: std::net::TcpStream,
}

impl TcpStream {
    /// Opens a TCP connection to `addr` with `TCP_NODELAY` enabled.
    pub fn connect(addr: SocketAddr) -> RS<Self> {
        let stream = std::net::TcpStream::connect(addr)
            .map_err(|e| network_error_with_message(e, "connect tcp stream error"))?;
        stream
            .set_nodelay(true)
            .map_err(|e| network_error_with_message(e, "set tcp nodelay error"))?;
        Ok(Self::from_inner(stream))
    }

    /// Wraps an existing [`std::net::TcpStream`].
    pub fn from_inner(inner: std::net::TcpStream) -> Self {
        Self { inner }
    }

    /// Enables or disables `TCP_NODELAY` (Nagle's algorithm).
    pub fn set_nodelay(&self, nodelay: bool) -> RS<()> {
        self.inner
            .set_nodelay(nodelay)
            .map_err(|e| network_error_with_message(e, "set tcp nodelay error"))
    }

    /// Returns the local address of this connection.
    pub fn local_addr(&self) -> RS<SocketAddr> {
        self.inner
            .local_addr()
            .map_err(|e| network_error_with_message(e, "read tcp local addr error"))
    }

    /// Returns the remote peer address of this connection.
    pub fn peer_addr(&self) -> RS<SocketAddr> {
        self.inner
            .peer_addr()
            .map_err(|e| network_error_with_message(e, "read tcp peer addr error"))
    }

    /// Unwraps into the underlying [`std::net::TcpStream`].
    pub fn into_inner(self) -> std::net::TcpStream {
        self.inner
    }
}

impl std::io::Read for TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        self.inner.read_exact(buf)
    }
}

impl std::io::Write for TcpStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.inner.write_all(buf)
    }
}

#[cfg(unix)]
impl AsRawFd for TcpStream {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
