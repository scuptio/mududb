#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[cfg(test)]
mod tests {
    use crate::contract::async_net::AsyncNet;
    use crate::contract::async_stream::AsyncStream;
    use crate::imp::net::async_::TokioNet;
    use crate::task::async_::spawn_task_detached;
    use mudu::common::result::RS;
    use mudu::error::ErrorCode;
    use mudu::mudu_error;

    async fn read_exact(stream: &mut dyn AsyncStream, len: usize) -> RS<Vec<u8>> {
        let mut buf = vec![0u8; len];
        let mut done = 0usize;
        while done < len {
            let n = stream.read(&mut buf[done..]).await?;
            if n == 0 {
                return Err(mudu_error!(
                    ErrorCode::Network,
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
        let listen = net.bind_tcp("127.0.0.1:0".parse().unwrap()).await.unwrap();

        let addr = listen.local_addr().unwrap();

        let accept_task = spawn_task_detached("test", async move {
            let (mut stream, _) = listen.accept().await.unwrap();
            let payload = read_exact(stream.as_mut(), 4).await.unwrap();
            assert_eq!(payload, b"ping");
            stream.write_all(b"pong").await.unwrap();
            stream.shutdown().await.unwrap();
        })
        .unwrap();

        let mut client = net.connect_tcp(addr).await.unwrap();
        client.write_all(b"ping").await.unwrap();
        let response = read_exact(client.as_mut(), 4).await.unwrap();
        assert_eq!(response, b"pong");
        client.shutdown().await.unwrap();
        accept_task.await.unwrap();
    }
    #[tokio::test]
    async fn backend_listener_rebind_fails_and_shutdown_signals_eof() {
        use super::super::{TokioTcpListener, TokioTcpStream};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = TokioTcpListener::bind("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        assert!(TokioTcpListener::bind(addr).await.is_err());
        let mut client = TokioTcpStream::connect(addr).await.unwrap();
        let (mut server, _) = listener.accept().await.unwrap();
        AsyncWriteExt::shutdown(&mut client).await.unwrap();
        let mut byte = [0];
        assert_eq!(server.read(&mut byte).await.unwrap(), 0);
        drop(listener.into_inner());
    }

    #[tokio::test]
    async fn unconnected_backend_stream_reports_missing_peer() {
        use super::super::TokioTcpStream;
        let socket = socket2::Socket::new(
            socket2::Domain::IPV4,
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )
        .unwrap();
        socket.set_nonblocking(true).unwrap();
        let mut stream = TokioTcpStream::from_std(socket.into()).unwrap();
        assert!(stream.peer_addr().is_err());
        stream.shutdown().await.unwrap();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn backend_rejects_non_ip_socket_addresses_and_tcp_options() {
        use super::super::{TokioTcpListener, TokioTcpStream};
        let socket =
            socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None).unwrap();
        socket.set_nonblocking(true).unwrap();
        let listener = TokioTcpListener::from_std(socket.into()).unwrap();
        assert!(listener.local_addr().is_err());
        let socket =
            socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None).unwrap();
        socket.set_nonblocking(true).unwrap();
        let stream = TokioTcpStream::from_std(socket.into()).unwrap();
        assert!(stream.local_addr().is_err());
        assert!(stream.set_nodelay(true).is_err());
    }
}
