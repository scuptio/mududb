use crate::async_rt::contract::{AsyncListener, AsyncNet, AsyncStream};
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use crate::tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::tokio::net::{TcpListener, TcpStream};
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Default)]
pub struct TokioNet;

impl TokioNet {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AsyncNet for TokioNet {
    async fn bind_tcp(&self, addr: SocketAddr) -> RS<Arc<dyn AsyncListener>> {
        let listener = TcpListener::bind(addr).await.map_err(|e| {
            m_error!(
                EC::NetErr,
                format!("bind tokio tcp listener error: {addr}"),
                e
            )
        })?;
        Ok(Arc::new(TokioListener { inner: listener }))
    }

    async fn connect_tcp(&self, addr: SocketAddr) -> RS<Box<dyn AsyncStream>> {
        let stream = TcpStream::connect(addr).await.map_err(|e| {
            m_error!(
                EC::NetErr,
                format!("connect tokio tcp stream error: {addr}"),
                e
            )
        })?;
        stream
            .set_nodelay(true)
            .map_err(|e| m_error!(EC::NetErr, format!("set tcp nodelay error: {addr}"), e))?;
        Ok(Box::new(TokioStream { inner: stream }))
    }
}

struct TokioListener {
    inner: TcpListener,
}

#[async_trait]
impl AsyncListener for TokioListener {
    fn local_addr(&self) -> RS<SocketAddr> {
        self.inner
            .local_addr()
            .map_err(|e| m_error!(EC::NetErr, "read tokio listener local addr error", e))
    }

    async fn accept(&self) -> RS<(Box<dyn AsyncStream>, SocketAddr)> {
        let (stream, addr) = self
            .inner
            .accept()
            .await
            .map_err(|e| m_error!(EC::NetErr, "accept tokio tcp stream error", e))?;
        stream
            .set_nodelay(true)
            .map_err(|e| m_error!(EC::NetErr, "set accepted tcp nodelay error", e))?;
        Ok((Box::new(TokioStream { inner: stream }), addr))
    }
}

struct TokioStream {
    inner: TcpStream,
}

#[async_trait]
impl AsyncStream for TokioStream {
    async fn read(&mut self, buf: &mut [u8]) -> RS<usize> {
        self.inner
            .read(buf)
            .await
            .map_err(|e| m_error!(EC::NetErr, "read tokio tcp stream error", e))
    }

    async fn write_all(&mut self, buf: &[u8]) -> RS<()> {
        self.inner
            .write_all(buf)
            .await
            .map_err(|e| m_error!(EC::NetErr, "write tokio tcp stream error", e))
    }

    async fn shutdown(&mut self) -> RS<()> {
        self.inner
            .shutdown()
            .await
            .map_err(|e| m_error!(EC::NetErr, "shutdown tokio tcp stream error", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn read_exact(stream: &mut dyn AsyncStream, len: usize) -> RS<Vec<u8>> {
        let mut buf = vec![0u8; len];
        let mut done = 0usize;
        while done < len {
            let n = stream.read(&mut buf[done..]).await?;
            if n == 0 {
                return Err(m_error!(
                    EC::NetErr,
                    "unexpected eof while reading exact bytes"
                ));
            }
            done += n;
        }
        Ok(buf)
    }

    #[tokio::test]
    async fn tokio_net_connects_and_transfers_bytes() {
        let net = TokioNet::new();
        let std_listener = match std::net::TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => return,
            Err(err) => panic!("bind tcp listener failed: {err}"),
        };
        std_listener.set_nonblocking(true).unwrap();
        let addr = std_listener.local_addr().unwrap();
        let listener = TokioListener {
            inner: TcpListener::from_std(std_listener).unwrap(),
        };

        let accept_task = crate::tokio::spawn(async move {
            let (mut stream, _) = AsyncListener::accept(&listener).await.unwrap();
            let payload = read_exact(stream.as_mut(), 4).await.unwrap();
            assert_eq!(payload, b"ping");
            stream.write_all(b"pong").await.unwrap();
            stream.shutdown().await.unwrap();
        });

        let mut client = net.connect_tcp(addr).await.unwrap();
        client.write_all(b"ping").await.unwrap();
        let response = read_exact(client.as_mut(), 4).await.unwrap();
        assert_eq!(response, b"pong");
        client.shutdown().await.unwrap();
        accept_task.await.unwrap();
    }
}
