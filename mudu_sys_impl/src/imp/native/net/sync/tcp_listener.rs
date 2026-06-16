use super::tcp_stream::TcpStream;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, IntoRawFd, RawFd};

pub struct TcpListener {
    inner: std::net::TcpListener,
}

impl TcpListener {
    pub fn bind(addr: SocketAddr) -> RS<Self> {
        let domain = if addr.is_ipv4() {
            Domain::IPV4
        } else {
            Domain::IPV6
        };
        let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
            .map_err(|e| m_error!(EC::NetErr, "create tcp listener socket error", e))?;
        socket
            .set_reuse_address(true)
            .map_err(|e| m_error!(EC::NetErr, "enable SO_REUSEADDR error", e))?;
        socket
            .bind(&addr.into())
            .map_err(|e| m_error!(EC::NetErr, format!("bind tcp listener error: {addr}"), e))?;
        socket
            .listen(1024)
            .map_err(|e| m_error!(EC::NetErr, "listen tcp listener error", e))?;
        Ok(Self::from_inner(std::net::TcpListener::from(socket)))
    }

    pub fn from_inner(inner: std::net::TcpListener) -> Self {
        Self { inner }
    }

    pub fn accept(&self) -> RS<(TcpStream, SocketAddr)> {
        self.inner
            .accept()
            .map_err(|e| m_error!(EC::NetErr, "accept tcp connection error", e))
            .map(|(stream, addr)| (TcpStream::from_inner(stream), addr))
    }

    pub fn local_addr(&self) -> RS<SocketAddr> {
        self.inner
            .local_addr()
            .map_err(|e| m_error!(EC::NetErr, "read tcp listener local addr error", e))
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> RS<()> {
        self.inner
            .set_nonblocking(nonblocking)
            .map_err(|e| m_error!(EC::NetErr, "set tcp listener nonblocking error", e))
    }

    pub fn try_clone(&self) -> RS<Self> {
        self.inner
            .try_clone()
            .map(Self::from_inner)
            .map_err(|e| m_error!(EC::NetErr, "clone tcp listener error", e))
    }

    pub fn into_inner(self) -> std::net::TcpListener {
        self.inner
    }

    pub fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}

impl AsRawFd for TcpListener {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
