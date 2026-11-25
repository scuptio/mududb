use std::net::SocketAddr;
use std::sync::Arc;

use mudu::error::ErrorCode;
use tokio::task::yield_now;

use crate::imp::native::linux::io_uring::worker_ring::{
    set_current_worker_ring, unset_current_worker_ring, WorkerLocalRing, WorkerRingOp,
};
use crate::task::async_::spawn_task_detached;

use super::{accept, close, connect, recv_into, send, shutdown, socket, IoSocket, SocketIoRequest};

#[allow(clippy::arc_with_non_send_sync)]
fn install_test_ring() -> Arc<WorkerLocalRing> {
    let ring = Arc::new(WorkerLocalRing::new());
    set_current_worker_ring(ring.clone());
    ring
}

#[tokio::test(flavor = "current_thread")]
async fn socket_and_connect_enqueue_requests() {
    let ring = install_test_ring();
    let create_task = spawn_task_detached("test", async {
        socket(libc::AF_INET, libc::SOCK_STREAM, 0).await
    })
    .unwrap();
    yield_now().await;
    match ring.take_pending().unwrap().unwrap().1 {
        WorkerRingOp::Socket(SocketIoRequest::Socket(request)) => {
            assert_eq!(request.domain(), libc::AF_INET);
            assert_eq!(request.socket_type(), libc::SOCK_STREAM);
            assert_eq!(request.protocol(), 0);
            request.finish(Ok(41));
        }
        _ => panic!("expected socket request"),
    }
    let sock = create_task.await.unwrap().unwrap().unwrap();
    assert_eq!(sock.fd(), 41);

    let connect_task = spawn_task_detached("test", async move {
        connect(&sock, "127.0.0.1:9527".parse().unwrap()).await
    })
    .unwrap();
    yield_now().await;
    match ring.take_pending().unwrap().unwrap().1 {
        WorkerRingOp::Socket(SocketIoRequest::Connect(request)) => {
            assert_eq!(request.fd(), 41);
            request.finish(Ok(()));
        }
        _ => panic!("expected connect request"),
    }
    connect_task.await.unwrap().unwrap().unwrap();
    unset_current_worker_ring();
}

#[tokio::test(flavor = "current_thread")]
async fn accept_recv_send_shutdown_and_close_enqueue_requests() {
    let ring = install_test_ring();
    let sock = IoSocket { fd: 51 };

    let accept_task = spawn_task_detached("test", async move { accept(&sock).await }).unwrap();
    yield_now().await;
    match ring.take_pending().unwrap().unwrap().1 {
        WorkerRingOp::Socket(SocketIoRequest::Accept(request)) => {
            assert_eq!(request.fd(), 51);
            request.finish(Ok((61, "127.0.0.1:9010".parse().unwrap())));
        }
        _ => panic!("expected accept request"),
    }
    let (accepted, addr) = accept_task.await.unwrap().unwrap().unwrap();
    assert_eq!(accepted.fd(), 61);
    assert_eq!(addr, "127.0.0.1:9010".parse::<SocketAddr>().unwrap());

    let recv_task = spawn_task_detached("test", async move {
        let mut buf = [0u8; 8];
        let read = recv_into(&accepted, &mut buf, libc::MSG_DONTWAIT).await?;
        Ok::<_, mudu::error::MuduError>((read, buf))
    })
    .unwrap();
    yield_now().await;
    match ring.take_pending().unwrap().unwrap().1 {
        WorkerRingOp::Socket(SocketIoRequest::Recv(request)) => {
            assert_eq!(request.fd(), 61);
            assert_eq!(request.len(), 8);
            assert_eq!(request.flags(), libc::MSG_DONTWAIT);
            unsafe {
                std::ptr::copy_nonoverlapping(
                    [7u8, 8, 9].as_ptr(),
                    request.buf_ptr() as *mut u8,
                    3,
                );
            }
            request.finish(Ok(3));
        }
        _ => panic!("expected recv request"),
    }
    let (read, recv_buf) = recv_task.await.unwrap().unwrap().unwrap();
    assert_eq!(read, 3);
    assert_eq!(&recv_buf[..3], &[7, 8, 9]);

    let send_sock = IoSocket { fd: 71 };
    let send_task = spawn_task_detached("test", async move {
        send(&send_sock, vec![1, 2, 3], libc::MSG_NOSIGNAL).await
    })
    .unwrap();
    yield_now().await;
    match ring.take_pending().unwrap().unwrap().1 {
        WorkerRingOp::Socket(SocketIoRequest::Send(request)) => {
            assert_eq!(request.fd(), 71);
            assert_eq!(request.flags(), libc::MSG_NOSIGNAL);
            assert_eq!(request.remaining_len(), 3);
            request.finish(Ok(3));
        }
        _ => panic!("expected send request"),
    }
    assert_eq!(send_task.await.unwrap().unwrap().unwrap(), 3);

    let shutdown_sock = IoSocket { fd: 71 };
    let shutdown_task = spawn_task_detached("test", async move {
        shutdown(&shutdown_sock, libc::SHUT_WR).await
    })
    .unwrap();
    yield_now().await;
    match ring.take_pending().unwrap().unwrap().1 {
        WorkerRingOp::Socket(SocketIoRequest::Shutdown(request)) => {
            assert_eq!(request.fd(), 71);
            assert_eq!(request.how(), libc::SHUT_WR);
            request.finish(Ok(()));
        }
        _ => panic!("expected shutdown request"),
    }
    shutdown_task.await.unwrap().unwrap().unwrap();

    let close_task =
        spawn_task_detached("test", async move { close(IoSocket { fd: 71 }).await }).unwrap();
    yield_now().await;
    match ring.take_pending().unwrap().unwrap().1 {
        WorkerRingOp::Socket(SocketIoRequest::Close(request)) => {
            assert_eq!(request.fd(), 71);
            request.finish(Ok(()));
        }
        _ => panic!("expected close request"),
    }
    close_task.await.unwrap().unwrap().unwrap();
    unset_current_worker_ring();
}

#[tokio::test(flavor = "current_thread")]
async fn socket_without_current_ring_returns_error() {
    unset_current_worker_ring();
    let err = socket(libc::AF_INET, libc::SOCK_STREAM, 0)
        .await
        .unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
}

#[test]
fn socket_full_tcp_roundtrip_through_real_ring() {
    use super::{recv, send_all};
    use crate::imp::native::linux::io_uring::test_ring::drive_iouring_future;
    use std::os::fd::IntoRawFd;

    let result = drive_iouring_future(async {
        // The listening socket comes from std; everything else goes through
        // the ring: open, connect, accept, send/recv both ways, shutdown, close.
        let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = std_listener.local_addr().unwrap();
        let listen_sock = IoSocket::from_raw_fd(std_listener.into_raw_fd());

        let client = socket(libc::AF_INET, libc::SOCK_STREAM, 0).await.unwrap();
        connect(&client, addr).await.unwrap();
        let (server_side, peer) = accept(&listen_sock).await.unwrap();
        assert!(peer.ip().is_loopback());

        let sent = send(&client, b"ping".to_vec(), 0).await.unwrap();
        assert_eq!(sent, 4);
        let data = recv(&server_side, 4, 0).await.unwrap();
        assert_eq!(data, b"ping");

        send_all(&server_side, b"pong").await.unwrap();
        let mut buf = [0u8; 4];
        let read = recv_into(&client, &mut buf, 0).await.unwrap();
        assert_eq!(read, 4);
        assert_eq!(&buf, b"pong");

        shutdown(&client, libc::SHUT_WR).await.unwrap();
        close(client).await.unwrap();
        close(server_side).await.unwrap();
        close(listen_sock).await.unwrap();
    });
    // `None` means io_uring is unavailable on this host; the harness logged it.
    let _ = result;
}

#[tokio::test]
async fn failed_socket_completions_propagate_and_do_not_requeue() {
    use super::request::{SocketCloseRequest, SocketInflightOp, SocketShutdownRequest};
    use super::ring_io::complete_socket_io;
    use crate::imp::io::user_io::{op_state, poll_op};
    let ring = WorkerLocalRing::new();
    let state = op_state();
    let request = SocketCloseRequest::new(-1, state.clone());
    assert!(complete_socket_io(
        1,
        SocketInflightOp::Close(Box::new(request)),
        -libc::EBADF,
        &ring
    )
    .unwrap());
    assert!(std::future::poll_fn(|cx| poll_op(&state, cx))
        .await
        .is_err());
    let state = op_state();
    let request = SocketShutdownRequest::new(-1, libc::SHUT_RDWR, state.clone());
    assert!(complete_socket_io(
        2,
        SocketInflightOp::Shutdown(Box::new(request)),
        -libc::ENOTCONN,
        &ring
    )
    .unwrap());
    assert!(std::future::poll_fn(|cx| poll_op(&state, cx))
        .await
        .is_err());
    assert!(ring.take_pending().unwrap().is_none());
}
