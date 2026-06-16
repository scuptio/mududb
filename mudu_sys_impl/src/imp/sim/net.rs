use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub mod sync {
    use mudu::common::result::RS;
    use mudu::error::ec::EC;
    use mudu::m_error;
    use std::net::SocketAddr;
    use std::os::fd::{AsRawFd, RawFd};

    pub struct TcpListener;

    pub struct TcpStream;

    impl TcpListener {
        pub fn accept(&self) -> RS<(TcpStream, SocketAddr)> {
            Err(m_error!(EC::NotImplemented, "[sim] TcpListener::accept"))
        }

        pub fn local_addr(&self) -> RS<SocketAddr> {
            Err(m_error!(
                EC::NotImplemented,
                "[sim] TcpListener::local_addr"
            ))
        }

        pub fn set_nonblocking(&self, _nonblocking: bool) -> RS<()> {
            Err(m_error!(
                EC::NotImplemented,
                "[sim] TcpListener::set_nonblocking"
            ))
        }

        pub fn try_clone(&self) -> RS<Self> {
            Err(m_error!(EC::NotImplemented, "[sim] TcpListener::try_clone"))
        }

        pub fn into_inner(self) -> std::net::TcpListener {
            panic!("[sim] TcpListener::into_inner is not available")
        }

        pub fn into_raw_fd(self) -> RawFd {
            panic!("[sim] TcpListener::into_raw_fd is not available")
        }
    }

    impl TcpStream {
        pub fn set_nodelay(&self, _nodelay: bool) -> RS<()> {
            Err(m_error!(EC::NotImplemented, "[sim] TcpStream::set_nodelay"))
        }

        pub fn local_addr(&self) -> RS<SocketAddr> {
            Err(m_error!(EC::NotImplemented, "[sim] TcpStream::local_addr"))
        }

        pub fn peer_addr(&self) -> RS<SocketAddr> {
            Err(m_error!(EC::NotImplemented, "[sim] TcpStream::peer_addr"))
        }

        pub fn into_inner(self) -> std::net::TcpStream {
            panic!("[sim] TcpStream::into_inner is not available")
        }
    }

    impl std::io::Read for TcpStream {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "[sim] TcpStream::read is not available",
            ))
        }
    }

    impl std::io::Write for TcpStream {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "[sim] TcpStream::write is not available",
            ))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "[sim] TcpStream::flush is not available",
            ))
        }
    }

    impl AsRawFd for TcpStream {
        fn as_raw_fd(&self) -> RawFd {
            panic!("[sim] TcpStream::as_raw_fd is not available")
        }
    }

    pub fn bind_tcp(addr: SocketAddr) -> RS<TcpListener> {
        crate::imp::sim::env::Sys::net().bind_tcp_sync(addr)
    }

    pub fn connect_tcp(addr: SocketAddr) -> RS<TcpStream> {
        crate::imp::sim::env::Sys::net().connect_tcp_sync(addr)
    }
}

pub mod async_ {
    use mudu::common::result::RS;
    use std::net::SocketAddr;

    pub use super::{TcpListener, TcpStream};

    pub async fn bind_tcp(addr: SocketAddr) -> RS<TcpListener> {
        crate::imp::sim::env::Sys::net().bind_tcp(addr).await
    }

    pub async fn connect_tcp(addr: SocketAddr) -> RS<TcpStream> {
        crate::imp::sim::env::Sys::net().connect_tcp(addr).await
    }

    pub async fn lookup_host<A: tokio::net::ToSocketAddrs>(addr: A) -> RS<Vec<SocketAddr>> {
        crate::imp::sim::env::Sys::net().lookup_host(addr).await
    }

    pub fn listener_from_std(listener: std::net::TcpListener) -> RS<TcpListener> {
        crate::imp::sim::env::Sys::net().listener_from_std(listener)
    }

    pub fn stream_from_std(stream: std::net::TcpStream) -> RS<TcpStream> {
        crate::imp::sim::env::Sys::net().stream_from_std(stream)
    }
}

pub struct TcpListener;

pub struct TcpStream;

impl TcpListener {
    pub async fn accept(&self) -> RS<(TcpStream, SocketAddr)> {
        Err(m_error!(EC::NotImplemented, "[sim] TcpListener::accept"))
    }

    pub fn local_addr(&self) -> RS<SocketAddr> {
        Err(m_error!(
            EC::NotImplemented,
            "[sim] TcpListener::local_addr"
        ))
    }

    pub fn into_inner(self) -> tokio::net::TcpListener {
        panic!("[sim] TcpListener::into_inner is not available")
    }
}

impl TcpStream {
    pub fn new(_inner: tokio::net::TcpStream) -> Self {
        Self
    }

    pub fn set_nodelay(&self, _nodelay: bool) -> RS<()> {
        Err(m_error!(EC::NotImplemented, "[sim] TcpStream::set_nodelay"))
    }

    pub fn local_addr(&self) -> RS<SocketAddr> {
        Err(m_error!(EC::NotImplemented, "[sim] TcpStream::local_addr"))
    }

    pub fn peer_addr(&self) -> RS<SocketAddr> {
        Err(m_error!(EC::NotImplemented, "[sim] TcpStream::peer_addr"))
    }

    pub async fn shutdown(&mut self) -> RS<()> {
        Err(m_error!(EC::NotImplemented, "[sim] TcpStream::shutdown"))
    }

    pub fn into_inner(self) -> tokio::net::TcpStream {
        panic!("[sim] TcpStream::into_inner is not available")
    }

    pub fn into_std(self) -> std::io::Result<std::net::TcpStream> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "[sim] TcpStream::into_std is not available",
        ))
    }
}

impl AsyncRead for TcpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Ready(Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "[sim] TcpStream::poll_read is not available",
        )))
    }
}

impl AsyncWrite for TcpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Poll::Ready(Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "[sim] TcpStream::poll_write is not available",
        )))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "[sim] TcpStream::poll_flush is not available",
        )))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "[sim] TcpStream::poll_shutdown is not available",
        )))
    }
}

impl AsRawFd for TcpStream {
    fn as_raw_fd(&self) -> RawFd {
        panic!("[sim] TcpStream::as_raw_fd is not available")
    }
}

impl AsRawFd for TcpListener {
    fn as_raw_fd(&self) -> RawFd {
        panic!("[sim] TcpListener::as_raw_fd is not available")
    }
}

#[derive(Default)]
pub struct Net;

impl Net {
    pub fn new() -> Self {
        Self
    }

    pub async fn bind_tcp(&self, _addr: SocketAddr) -> RS<TcpListener> {
        Err(m_error!(EC::NotImplemented, "[sim] Net::bind_tcp"))
    }

    pub async fn connect_tcp(&self, _addr: SocketAddr) -> RS<TcpStream> {
        Err(m_error!(EC::NotImplemented, "[sim] Net::connect_tcp"))
    }

    pub fn bind_tcp_sync(&self, _addr: SocketAddr) -> RS<sync::TcpListener> {
        Err(m_error!(EC::NotImplemented, "[sim] Net::bind_tcp_sync"))
    }

    pub fn connect_tcp_sync(&self, _addr: SocketAddr) -> RS<sync::TcpStream> {
        Err(m_error!(EC::NotImplemented, "[sim] Net::connect_tcp_sync"))
    }

    pub async fn lookup_host<A: tokio::net::ToSocketAddrs>(&self, _addr: A) -> RS<Vec<SocketAddr>> {
        Err(m_error!(EC::NotImplemented, "[sim] Net::lookup_host"))
    }

    pub fn listener_from_std(&self, _listener: std::net::TcpListener) -> RS<TcpListener> {
        Err(m_error!(EC::NotImplemented, "[sim] Net::listener_from_std"))
    }

    pub fn stream_from_std(&self, _stream: std::net::TcpStream) -> RS<TcpStream> {
        Err(m_error!(EC::NotImplemented, "[sim] Net::stream_from_std"))
    }
}
