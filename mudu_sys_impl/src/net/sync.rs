use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::net::SocketAddr;
use std::os::fd::AsRawFd;

pub struct StdTcpListener(crate::imp::net::sync::TcpListenerSync);
pub struct SStdTcpStream(crate::imp::net::sync::TcpStreamSync);

impl StdTcpListener {
    pub fn bind(addr: SocketAddr) -> RS<Self> {
        bind_tcp(addr)
    }

    pub fn accept(&self) -> RS<(SStdTcpStream, SocketAddr)> {
        self.0.accept().map(|(s, a)| (SStdTcpStream(s), a))
    }

    pub fn local_addr(&self) -> RS<SocketAddr> {
        self.0.local_addr()
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> RS<()> {
        self.0.set_nonblocking(nonblocking)
    }

    pub fn try_clone(&self) -> RS<Self> {
        self.0.try_clone().map(Self)
    }

    pub fn into_inner(self) -> std::net::TcpListener {
        self.0.into_inner()
    }

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
    pub fn connect<A: std::net::ToSocketAddrs>(addr: A) -> RS<Self> {
        let addrs: Vec<SocketAddr> = addr
            .to_socket_addrs()
            .map_err(|e| m_error!(EC::NetErr, "resolve connect address error", e))?
            .collect();
        if addrs.is_empty() {
            return Err(m_error!(EC::NetErr, "no addresses to connect"));
        }
        crate::imp::net::sync::connect_tcp(addrs[0]).map(Self)
    }

    pub fn set_nodelay(&self, nodelay: bool) -> RS<()> {
        self.0.set_nodelay(nodelay)
    }

    pub fn local_addr(&self) -> RS<SocketAddr> {
        self.0.local_addr()
    }

    pub fn peer_addr(&self) -> RS<SocketAddr> {
        self.0.peer_addr()
    }

    pub fn into_inner(self) -> std::net::TcpStream {
        self.0.into_inner()
    }
}

impl AsRawFd for SStdTcpStream {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.0.as_raw_fd()
    }
}

pub trait SyncNet: Send + Sync {
    fn bind_tcp(&self, addr: SocketAddr) -> RS<StdTcpListener>;
    fn connect_tcp(&self, addr: SocketAddr) -> RS<SStdTcpStream>;
}

pub fn bind_tcp(addr: SocketAddr) -> RS<StdTcpListener> {
    crate::imp::net::sync::bind_tcp(addr).map(StdTcpListener)
}

pub fn connect_tcp(addr: SocketAddr) -> RS<SStdTcpStream> {
    crate::imp::net::sync::connect_tcp(addr).map(SStdTcpStream)
}

pub struct StdSyncNet;

impl SyncNet for StdSyncNet {
    fn bind_tcp(&self, addr: SocketAddr) -> RS<StdTcpListener> {
        StdTcpListener::bind(addr)
    }

    fn connect_tcp(&self, addr: SocketAddr) -> RS<SStdTcpStream> {
        SStdTcpStream::connect(addr)
    }
}

impl crate::contract::net::SyncNet for StdSyncNet {
    fn bind_tcp(&self, addr: SocketAddr) -> RS<std::net::TcpListener> {
        StdTcpListener::bind(addr).map(StdTcpListener::into_inner)
    }

    fn connect_tcp(&self, addr: SocketAddr) -> RS<std::net::TcpStream> {
        SStdTcpStream::connect(addr).map(SStdTcpStream::into_inner)
    }
}
