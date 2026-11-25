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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{bind_tcp, connect_tcp, StdSyncNet, StdTcpListener, SyncNet};
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;

    #[test]
    fn sync_tcp_loopback_roundtrip() {
        let listener = bind_tcp("127.0.0.1:0".parse().unwrap()).unwrap();
        listener.set_nonblocking(true).unwrap();
        listener.set_nonblocking(false).unwrap();
        let cloned = listener.try_clone().unwrap();
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            let (mut stream, _peer) = cloned.accept().unwrap();
            let mut buf = [0u8; 5];
            stream.read_exact(&mut buf).unwrap();
            stream.write_all(&buf).unwrap();
            stream.flush().unwrap();
            let extra = stream.write(b"x").unwrap();
            assert_eq!(extra, 1);
            stream
        });

        let mut client = connect_tcp(addr).unwrap();
        client.set_nodelay(true).unwrap();
        let _local = client.local_addr().unwrap();
        let _peer = client.peer_addr().unwrap();
        let written = client.write(b"hello").unwrap();
        assert_eq!(written, 5);
        let mut buf = [0u8; 5];
        client.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"hello");
        // One more pending byte exercises the plain `read` path.
        let mut one = [0u8; 1];
        assert_eq!(client.read(&mut one).unwrap(), 1);

        let _fd = client.as_raw_fd();
        let server_stream = server.join().unwrap();
        let _std_server_stream = server_stream.into_inner();
        let _std_client_stream = client.into_inner();
        let _std_listener = listener.into_inner();
    }

    #[cfg(unix)]
    #[test]
    fn listener_into_raw_fd() {
        let listener = StdTcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let fd = listener.into_raw_fd();
        unsafe { libc::close(fd) };
    }

    #[test]
    fn std_sync_net_both_trait_impls() {
        let net = StdSyncNet;
        let listener = SyncNet::bind_tcp(&net, "127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = listener.local_addr().unwrap();
        let _stream = SyncNet::connect_tcp(&net, addr).unwrap();

        let std_listener = <StdSyncNet as crate::net::contract::SyncNet>::bind_tcp(
            &net,
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let std_addr = std_listener.local_addr().unwrap();
        let _std_stream =
            <StdSyncNet as crate::net::contract::SyncNet>::connect_tcp(&net, std_addr).unwrap();
    }

    #[test]
    fn connect_rejects_unresolvable_address() {
        // A port above 65535 fails resolution locally without any DNS query.
        assert!(super::SStdTcpStream::connect("127.0.0.1:99999").is_err());
    }
}
