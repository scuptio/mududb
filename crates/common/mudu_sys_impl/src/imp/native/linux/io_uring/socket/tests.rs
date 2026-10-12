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
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);
}
