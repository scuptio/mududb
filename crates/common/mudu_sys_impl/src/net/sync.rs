use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::fd::AsRawFd;

/// Blocking TCP listener over the native sync backend.
pub struct StdTcpListener(crate::imp::net::sync::TcpListenerSync);
/// Blocking TCP stream implementing [`std::io::Read`] and
/// [`std::io::Write`].
pub struct SStdTcpStream(crate::imp::net::sync::TcpStreamSync);

impl StdTcpListener {
    /// Binds a blocking TCP listener to `addr` (with `SO_REUSEADDR`
    /// enabled).
    pub fn bind(addr: SocketAddr) -> RS<Self> {
        bind_tcp(addr)
    }

    /// Accepts a new connection, returning the stream and peer address.
    pub fn accept(&self) -> RS<(SStdTcpStream, SocketAddr)> {
        self.0.accept().map(|(s, a)| (SStdTcpStream(s), a))
    }

    /// Returns the local address this listener is bound to.
    pub fn local_addr(&self) -> RS<SocketAddr> {
        self.0.local_addr()
    }

    /// Switches the listener between blocking and non-blocking mode.
    pub fn set_nonblocking(&self, nonblocking: bool) -> RS<()> {
        self.0.set_nonblocking(nonblocking)
    }

    /// Creates a new handle referring to the same listener socket.
    pub fn try_clone(&self) -> RS<Self> {
        self.0.try_clone().map(Self)
    }

    /// Unwraps into the underlying [`std::net::TcpListener`].
    pub fn into_inner(self) -> std::net::TcpListener {
        self.0.into_inner()
    }

    /// Consumes the listener and returns the raw file descriptor (Unix
    /// only).
    #[cfg(unix)]
    pub fn into_raw_fd(self) -> std::os::fd::RawFd {
        self.0.into_raw_fd()
    }
}

impl std::io::Read for SStdTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        self.0.read_exact(buf)
    }
}

impl std::io::Write for SStdTcpStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.0.write_all(buf)
    }
}

impl SStdTcpStream {
    /// Resolves `addr` and opens a blocking TCP connection to the first
    /// resolved address (with `TCP_NODELAY` enabled).
    pub fn connect<A: std::net::ToSocketAddrs>(addr: A) -> RS<Self> {
        let addrs: Vec<SocketAddr> = addr
            .to_socket_addrs()
            .map_err(|e| mudu_error!(ErrorCode::Network, "resolve connect address error", e))?
            .collect();
        if addrs.is_empty() {
            return Err(mudu_error!(ErrorCode::Network, "no addresses to connect"));
        }
        crate::imp::net::sync::connect_tcp(addrs[0]).map(Self)
    }

    /// Enables or disables `TCP_NODELAY` (Nagle's algorithm).
    pub fn set_nodelay(&self, nodelay: bool) -> RS<()> {
        self.0.set_nodelay(nodelay)
    }

    /// Returns the local address of this connection.
    pub fn local_addr(&self) -> RS<SocketAddr> {
        self.0.local_addr()
    }

    /// Returns the remote peer address of this connection.
    pub fn peer_addr(&self) -> RS<SocketAddr> {
        self.0.peer_addr()
    }

    /// Unwraps into the underlying [`std::net::TcpStream`].
    pub fn into_inner(self) -> std::net::TcpStream {
        self.0.into_inner()
    }
}

#[cfg(unix)]
impl AsRawFd for SStdTcpStream {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.0.as_raw_fd()
    }
}

/// Blocking TCP networking contract over the wrapped [`StdTcpListener`] /
/// [`SStdTcpStream`] types.
pub trait SyncNet: Send + Sync {
    /// Binds a TCP listener to `addr`.
    fn bind_tcp(&self, addr: SocketAddr) -> RS<StdTcpListener>;
    /// Opens a TCP connection to `addr`.
    fn connect_tcp(&self, addr: SocketAddr) -> RS<SStdTcpStream>;
}

/// Binds a blocking TCP listener to `addr` (with `SO_REUSEADDR` enabled).
pub fn bind_tcp(addr: SocketAddr) -> RS<StdTcpListener> {
    crate::imp::net::sync::bind_tcp(addr).map(StdTcpListener)
}

/// Opens a blocking TCP connection to `addr` (with `TCP_NODELAY` enabled).
pub fn connect_tcp(addr: SocketAddr) -> RS<SStdTcpStream> {
    crate::imp::net::sync::connect_tcp(addr).map(SStdTcpStream)
}

/// Default [`SyncNet`] implementation backed by the native sync backend.
pub struct StdSyncNet;

impl SyncNet for StdSyncNet {
    fn bind_tcp(&self, addr: SocketAddr) -> RS<StdTcpListener> {
        StdTcpListener::bind(addr)
    }

    fn connect_tcp(&self, addr: SocketAddr) -> RS<SStdTcpStream> {
        SStdTcpStream::connect(addr)
    }
}

impl crate::net::contract::SyncNet for StdSyncNet {
    fn bind_tcp(&self, addr: SocketAddr) -> RS<std::net::TcpListener> {
        StdTcpListener::bind(addr).map(StdTcpListener::into_inner)
    }

    fn connect_tcp(&self, addr: SocketAddr) -> RS<std::net::TcpStream> {
        SStdTcpStream::connect(addr).map(SStdTcpStream::into_inner)
    }
}
