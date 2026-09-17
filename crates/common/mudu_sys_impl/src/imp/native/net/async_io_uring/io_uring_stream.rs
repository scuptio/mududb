use crate::imp::native::linux::io_uring::socket;
use crate::imp::native::linux::io_uring::socket::IoSocket;
use crate::imp::native::net::async_io_uring::socket_opt;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::net::SocketAddr;

pub(crate) struct IoUringStream {
    socket: IoSocket,
}

impl IoUringStream {
    pub(crate) async fn connect(addr: SocketAddr) -> RS<Self> {
        #[cfg(target_os = "linux")]
        if crate::imp::io::worker_ring::has_current_worker_ring() {
            let sock =
                socket::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0).await?;
            socket::connect(&sock, addr).await?;
            socket_opt::set_nodelay_fd(sock.fd())?;
            Ok(IoUringStream::new(sock))
        } else {
            Err(mudu_error!(
                ErrorCode::Internal,
                "io_uring stream connect requires a current worker ring"
            ))
        }
    }

    pub(crate) fn new(socket: IoSocket) -> Self {
        Self { socket }
    }

    pub(crate) async fn read(&self, buf: &mut [u8]) -> RS<usize> {
        socket::recv_into(&self.socket, buf, 0).await
    }

    pub(crate) async fn write_all(&mut self, buf: &[u8]) -> RS<()> {
        socket::send_all(&self.socket, buf).await
    }

    pub(crate) async fn shutdown(&mut self) -> RS<()> {
        socket::shutdown(&self.socket, libc::SHUT_RDWR).await
    }

    pub(crate) fn as_raw_fd(&self) -> Option<std::os::fd::RawFd> {
        Some(self.socket.fd())
    }

    pub(crate) fn set_nodelay(&self) -> RS<()> {
        socket_opt::set_nodelay_fd(self.socket.fd())
    }
}
