//! Drives the io_uring TCP provider (`AsyncIoUringNet`) end-to-end through a
//! real ring: bind, connect, accept, read/write both ways, shutdown.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(missing_docs)]

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use crate::contract::async_net::AsyncNet;
    use crate::imp::native::linux::io_uring::test_ring::drive_iouring_future;
    use crate::imp::net::async_io_uring::AsyncIoUringNet;

    #[test]
    fn io_uring_net_full_tcp_roundtrip() {
        let result = drive_iouring_future(async {
            let net = AsyncIoUringNet::new();
            let listener = net.bind_tcp("127.0.0.1:0".parse().unwrap()).await.unwrap();
            let addr = listener.local_addr().unwrap();
            let _fd = listener.as_raw_fd();
            let cloned = listener.try_clone_listener().unwrap();
            let _cloned_addr = cloned.local_addr().unwrap();

            let mut client = net.connect_tcp(addr).await.unwrap();
            client.set_nodelay().unwrap();
            let _fd = client.as_raw_fd();

            let (mut server, peer) = listener.accept().await.unwrap();
            assert!(peer.ip().is_loopback());

            client.write_all(b"ping").await.unwrap();
            let mut buf = [0u8; 4];
            let mut done = 0;
            while done < 4 {
                done += server.read(&mut buf[done..]).await.unwrap();
            }
            assert_eq!(&buf, b"ping");

            server.write_all(b"pong").await.unwrap();
            let mut buf = [0u8; 4];
            let mut done = 0;
            while done < 4 {
                done += client.read(&mut buf[done..]).await.unwrap();
            }
            assert_eq!(&buf, b"pong");

            client.shutdown().await.unwrap();
            server.shutdown().await.unwrap();
        });
        // `None` means io_uring is unavailable on this host; the harness logged it.
        let _ = result;
    }
}
