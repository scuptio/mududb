use crate::imp::native::linux::io_uring::socket;
use crate::imp::native::linux::io_uring::socket::IoSocket;
use crate::imp::net::async_io_uring::io_uring_stream::IoUringStream;
use crate::imp::net::async_io_uring::socket_opt;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{SocketAddr, TcpListener};
use std::os::fd::AsRawFd;

pub struct IoUringListener {
    inner: TcpListener,
}

impl IoUringListener {
    pub(crate) fn bind_listener(addr: SocketAddr) -> RS<Self> {
        let domain = if addr.is_ipv4() {
            Domain::IPV4
        } else {
            Domain::IPV6
        };
        let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
            .map_err(|e| mudu_error!(ErrorCode::Network, "create tcp listener socket error", e))?;
        socket
            .set_reuse_address(true)
            .map_err(|e| mudu_error!(ErrorCode::Network, "enable SO_REUSEADDR error", e))?;
        socket.bind(&addr.into()).map_err(|e| {
            mudu_error!(
                ErrorCode::Network,
                format!("bind tcp listener error: {addr}"),
                e
            )
        })?;
        socket
            .listen(128)
            .map_err(|e| mudu_error!(ErrorCode::Network, "listen tcp listener error", e))?;
        let listener = TcpListener::from(socket);
        Ok(IoUringListener::new(listener))
    }

    pub(crate) fn new(socket: TcpListener) -> Self {
        Self { inner: socket }
    }

    pub(crate) fn local_addr(&self) -> RS<SocketAddr> {
        self.inner
            .local_addr()
            .map_err(|e| mudu_error!(ErrorCode::Network, "read tcp listener local addr error", e))
    }

    pub(crate) async fn accept(&self) -> RS<(IoUringStream, SocketAddr)> {
        #[cfg(target_os = "linux")]
        if crate::imp::io::worker_ring::has_current_worker_ring() {
            // `IoSocket::from_raw_fd` is a non-owning view: the listening fd
            // stays owned by `self.inner` and must survive further accepts.
            let fd = self.inner.as_raw_fd();
            let (sock, addr) = socket::accept(&IoSocket::from_raw_fd(fd)).await?;
            socket_opt::set_nodelay_fd(sock.fd())?;
            Ok((IoUringStream::new(sock), addr))
        } else {
            Err(mudu_error!(
                ErrorCode::Internal,
                "io_uring accept requires a current worker ring"
            ))
        }
    }

    pub(crate) fn as_raw_fd(&self) -> Option<std::os::fd::RawFd> {
        Some(self.inner.as_raw_fd())
    }

    pub(crate) fn try_clone_listener(&self) -> RS<Self> {
        let cloned = self
            .inner
            .try_clone()
            .map_err(|e| mudu_error!(ErrorCode::Network, "clone io_uring tcp listener error", e))?;
        Ok(IoUringListener::new(cloned))
    }
}

#[cfg(all(test, target_os = "linux"))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::IoUringListener;

    #[test]
    fn bind_ipv6_and_rebind_error() {
        let v6 = IoUringListener::bind_listener("[::1]:0".parse().unwrap()).unwrap();
        assert!(v6.local_addr().unwrap().is_ipv6());

        let v4 = IoUringListener::bind_listener("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = v4.local_addr().unwrap();
        // A second bind on an already-listening address fails with EADDRINUSE.
        assert!(IoUringListener::bind_listener(addr).is_err());
    }

    #[test]
    fn accept_without_worker_ring_returns_error() {
        let listener = IoUringListener::bind_listener("127.0.0.1:0".parse().unwrap()).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime.block_on(listener.accept());
        assert!(result.is_err());
    }
}
