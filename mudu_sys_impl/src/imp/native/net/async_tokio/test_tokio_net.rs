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
}
