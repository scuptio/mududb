use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, RawFd};

pub struct TcpStream {
    inner: std::net::TcpStream,
}

impl TcpStream {
    pub fn connect(addr: SocketAddr) -> RS<Self> {
        let stream = std::net::TcpStream::connect(addr)
            .map_err(|e| m_error!(EC::NetErr, "connect tcp stream error", e))?;
        stream
            .set_nodelay(true)
            .map_err(|e| m_error!(EC::NetErr, "set tcp nodelay error", e))?;
        Ok(Self::from_inner(stream))
    }

    pub fn from_inner(inner: std::net::TcpStream) -> Self {
        Self { inner }
    }

    pub fn set_nodelay(&self, nodelay: bool) -> RS<()> {
        self.inner
            .set_nodelay(nodelay)
            .map_err(|e| m_error!(EC::NetErr, "set tcp nodelay error", e))
    }

    pub fn local_addr(&self) -> RS<SocketAddr> {
        self.inner
            .local_addr()
            .map_err(|e| m_error!(EC::NetErr, "read tcp local addr error", e))
    }

    pub fn peer_addr(&self) -> RS<SocketAddr> {
        self.inner
            .peer_addr()
            .map_err(|e| m_error!(EC::NetErr, "read tcp peer addr error", e))
    }

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

impl AsRawFd for TcpStream {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
