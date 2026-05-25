use crate::async_rt::contract::{AsyncListener, AsyncNet, AsyncStream};
use crate::io::socket::{self, IoSocket};
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::fd::IntoRawFd;
use std::sync::Arc;

#[derive(Default)]
pub struct IoUringNet;

impl IoUringNet {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AsyncNet for IoUringNet {
    async fn bind_tcp(&self, addr: SocketAddr) -> RS<Arc<dyn AsyncListener>> {
        let listener = TcpListener::bind(addr)
            .map_err(|e| m_error!(EC::NetErr, format!("bind tcp listener error: {addr}"), e))?;
        Ok(Arc::new(IoUringListener { inner: listener }))
    }

    async fn connect_tcp(&self, addr: SocketAddr) -> RS<Box<dyn AsyncStream>> {
        #[cfg(target_os = "linux")]
        if crate::io::worker_ring::has_current_worker_ring() {
            let sock =
                socket::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0).await?;
            socket::connect(&sock, addr).await?;
            set_nodelay_fd(sock.fd())?;
            return Ok(Box::new(IoUringStream::Ring(sock)));
        }

        let stream = TcpStream::connect(addr)
            .map_err(|e| m_error!(EC::NetErr, format!("connect tcp stream error: {addr}"), e))?;
        stream
            .set_nodelay(true)
            .map_err(|e| m_error!(EC::NetErr, format!("set tcp nodelay error: {addr}"), e))?;
        Ok(Box::new(IoUringStream::Std(stream)))
    }
}

struct IoUringListener {
    inner: TcpListener,
}

#[async_trait]
impl AsyncListener for IoUringListener {
    fn local_addr(&self) -> RS<SocketAddr> {
        self.inner
            .local_addr()
            .map_err(|e| m_error!(EC::NetErr, "read tcp listener local addr error", e))
    }

    async fn accept(&self) -> RS<(Box<dyn AsyncStream>, SocketAddr)> {
        #[cfg(target_os = "linux")]
        if crate::io::worker_ring::has_current_worker_ring() {
            let listener = self.inner.try_clone().map_err(|e| {
                m_error!(
                    EC::NetErr,
                    "clone tcp listener for io_uring accept error",
                    e
                )
            })?;
            let fd = listener.into_raw_fd();
            let (sock, addr) = socket::accept(&IoSocket::from_raw_fd(fd)).await?;
            let _ = mudu_sys::sync_sync::close_fd(fd);
            set_nodelay_fd(sock.fd())?;
            return Ok((Box::new(IoUringStream::Ring(sock)), addr));
        }

        let (stream, addr) = self
            .inner
            .accept()
            .map_err(|e| m_error!(EC::NetErr, "accept tcp stream error", e))?;
        stream
            .set_nodelay(true)
            .map_err(|e| m_error!(EC::NetErr, "set accepted tcp nodelay error", e))?;
        Ok((Box::new(IoUringStream::Std(stream)), addr))
    }
}

enum IoUringStream {
    Ring(IoSocket),
    Std(TcpStream),
}

#[async_trait]
impl AsyncStream for IoUringStream {
    async fn read(&mut self, buf: &mut [u8]) -> RS<usize> {
        match self {
            IoUringStream::Ring(sock) => socket::recv_into(sock, buf, 0).await,
            IoUringStream::Std(stream) => stream
                .read(buf)
                .map_err(|e| m_error!(EC::NetErr, "read tcp stream error", e)),
        }
    }

    async fn write_all(&mut self, buf: &[u8]) -> RS<()> {
        match self {
            IoUringStream::Ring(sock) => socket::send_all(sock, buf).await,
            IoUringStream::Std(stream) => stream
                .write_all(buf)
                .map_err(|e| m_error!(EC::NetErr, "write tcp stream error", e)),
        }
    }

    async fn shutdown(&mut self) -> RS<()> {
        match self {
            IoUringStream::Ring(sock) => socket::shutdown(sock, libc::SHUT_RDWR).await,
            IoUringStream::Std(stream) => stream
                .shutdown(std::net::Shutdown::Both)
                .map_err(|e| m_error!(EC::NetErr, "shutdown tcp stream error", e)),
        }
    }
}

fn set_nodelay_fd(fd: std::os::fd::RawFd) -> RS<()> {
    let flag: libc::c_int = 1;
    let rc = unsafe {
        libc::setsockopt(
            fd,
            libc::IPPROTO_TCP,
            libc::TCP_NODELAY,
            &flag as *const _ as *const libc::c_void,
            std::mem::size_of_val(&flag) as libc::socklen_t,
        )
    };
    if rc != 0 {
        return Err(m_error!(
            EC::NetErr,
            "set tcp nodelay on raw fd error",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}
