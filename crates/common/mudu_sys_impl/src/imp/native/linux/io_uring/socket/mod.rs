use std::net::SocketAddr;
use std::os::fd::RawFd;

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

mod future;
mod request;
mod ring_io;
#[cfg(all(test, target_os = "linux"))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;

use future::{
    SocketAcceptFuture, SocketCloseFuture, SocketConnectFuture, SocketOpenFuture,
    SocketRecvIntoFuture, SocketSendFuture, SocketSendRefFuture, SocketShutdownFuture,
};
pub use request::{
    SocketAcceptRequest, SocketCloseRequest, SocketConnectRequest, SocketInflightOp,
    SocketIoRequest, SocketOpenRequest, SocketRecvRequest, SocketSendRefRequest, SocketSendRequest,
    SocketShutdownRequest,
};
pub use ring_io::{complete_socket_io, submit_socket_io};

/// An open socket fd used with the io_uring worker ring.
#[derive(Debug)]
pub struct IoSocket {
    fd: RawFd,
}

/// Creates a socket through the current worker ring.
pub async fn socket(domain: i32, socket_type: i32, protocol: i32) -> RS<IoSocket> {
    let fd = SocketOpenFuture::new(domain, socket_type, protocol).await?;
    Ok(IoSocket { fd })
}

/// Connects `sock` to `addr` through the current worker ring.
pub async fn connect(sock: &IoSocket, addr: SocketAddr) -> RS<()> {
    let raw_addr = socket_addr_to_raw(addr)?;
    SocketConnectFuture::new(sock.fd, raw_addr).await
}

/// Accepts a connection on listening socket `sock`, returning the new
/// socket and peer address.
pub async fn accept(sock: &IoSocket) -> RS<(IoSocket, SocketAddr)> {
    let (fd, addr) = SocketAcceptFuture::new(sock.fd).await?;
    Ok((IoSocket { fd }, addr))
}

/// Receives up to `len` bytes from `sock` with recv `flags`.
pub async fn recv(sock: &IoSocket, len: usize, flags: i32) -> RS<Vec<u8>> {
    let mut buf = vec![0u8; len];
    let read = recv_into(sock, buf.as_mut_slice(), flags).await?;
    buf.truncate(read);
    Ok(buf)
}

/// Sends `data` on `sock` with send `flags`, returning the number of bytes
/// sent.
pub async fn send(sock: &IoSocket, data: Vec<u8>, flags: i32) -> RS<usize> {
    SocketSendFuture::new(sock.fd, data, flags).await
}

/// Shuts down one or both directions of `sock` (`SHUT_RD`/`SHUT_WR`/
/// `SHUT_RDWR`).
pub async fn shutdown(sock: &IoSocket, how: i32) -> RS<()> {
    SocketShutdownFuture::new(sock.fd, how).await
}

/// Closes `sock` through the current worker ring.
pub async fn close(sock: IoSocket) -> RS<()> {
    SocketCloseFuture::new(sock.fd).await
}

/// Receives from `sock` into `buf` with recv `flags`, returning the number
/// of bytes read.
pub async fn recv_into(sock: &IoSocket, buf: &mut [u8], flags: i32) -> RS<usize> {
    SocketRecvIntoFuture::new(sock.fd, buf, flags).await
}

/// Sends all of `data` on `sock`; fails if the ring reports a short write.
pub async fn send_all(sock: &IoSocket, data: &[u8]) -> RS<()> {
    let sent = SocketSendRefFuture::new(sock.fd, data, 0).await?;
    if sent != data.len() {
        return Err(mudu_error!(
            ErrorCode::Network,
            format!(
                "socket send incomplete: sent {}, expected {}",
                sent,
                data.len()
            )
        ));
    }
    Ok(())
}

impl IoSocket {
    /// Returns the raw socket file descriptor.
    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Wraps a raw socket file descriptor.
    pub fn from_raw_fd(fd: RawFd) -> Self {
        Self { fd }
    }
}

fn socket_addr_to_raw(
    addr: SocketAddr,
) -> RS<crate::imp::native::linux::io_uring::iouring::SockAddrBuf> {
    crate::imp::io::net::socket_addr_to_storage(addr).map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            "socket addr to storage conversion failed",
            e
        )
    })
}

/// Converts a raw `SockAddrBuf` back into a [`SocketAddr`].
pub fn raw_to_socket_addr(
    addr: &crate::imp::native::linux::io_uring::iouring::SockAddrBuf,
) -> RS<SocketAddr> {
    crate::imp::io::net::sockaddr_to_socket_addr(addr)
}
