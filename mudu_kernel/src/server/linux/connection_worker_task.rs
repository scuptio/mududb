use crate::io::socket::{close, IoSocket};
use crate::server::async_func_task::{HandleResult, SessionTransferDispatch};
use crate::server::frame_dispatch::dispatch_frame_async;
use crate::server::protocol_codec::{read_next_frame, write_response};
use crate::server::routing::ConnectionTransfer;
use crate::server::transferred_connection::TransferredConnection;
use crate::server::worker::WorkerRuntime;
use crate::server::worker_mailbox::WorkerMailboxMsg;
use crate::server::worker_ring_loop::WorkerRingLoop;
use crate::server::worker_task::WorkerTaskFuture;
use crossbeam_queue::SegQueue;
use mudu::common::result::RS;
use mudu_contract::protocol::encode_merror_response;
use std::net::SocketAddr;
use std::os::fd::RawFd;
use std::sync::Arc;
use tracing::trace;

pub(in crate::server) fn spawn_connection_worker_task(
    worker: WorkerRuntime,
    mailbox_fds: Vec<RawFd>,
    mailboxes: Vec<Arc<SegQueue<WorkerMailboxMsg>>>,
    connections: Arc<scc::HashMap<u64, RawFd>>,
    conn_id: u64,
    socket: IoSocket,
    remote_addr: SocketAddr,
    initial_response: Option<Vec<u8>>,
) -> WorkerTaskFuture {
    Box::pin(async move {
        run_connection_worker_task(
            worker,
            mailbox_fds,
            mailboxes,
            connections,
            conn_id,
            socket,
            remote_addr,
            initial_response,
        )
        .await
    })
}

async fn run_connection_worker_task(
    worker: WorkerRuntime,
    mailbox_fds: Vec<RawFd>,
    mailboxes: Vec<Arc<SegQueue<WorkerMailboxMsg>>>,
    connections: Arc<scc::HashMap<u64, RawFd>>,
    conn_id: u64,
    socket: IoSocket,
    remote_addr: SocketAddr,
    initial_response: Option<Vec<u8>>,
) -> RS<()> {
    mudu_utils::scoped_task_trace!();
    let r = _run_connection_worker_task(
        worker,
        mailbox_fds,
        mailboxes,
        conn_id,
        socket,
        remote_addr,
        initial_response,
    )
    .await;
    let _ = connections.remove_sync(&conn_id);
    r
}
async fn _run_connection_worker_task(
    worker: WorkerRuntime,
    mailbox_fds: Vec<RawFd>,
    mailboxes: Vec<Arc<SegQueue<WorkerMailboxMsg>>>,
    conn_id: u64,
    socket: IoSocket,
    remote_addr: SocketAddr,
    initial_response: Option<Vec<u8>>,
) -> RS<()> {
    mudu_utils::scoped_task_trace!();
    let mut socket = Some(socket);
    let mut read_buf = Vec::with_capacity(8192);
    trace!(
        conn_id,
        remote_addr = %remote_addr,
        "io_uring connection worker started"
    );

    if let Some(response) = initial_response {
        trace!(
            conn_id,
            bytes = response.len(),
            "sending initial connection response"
        );
        write_response(socket.as_ref().unwrap(), &response).await?;
    }

    loop {
        trace!(conn_id, "waiting for next protocol frame");
        let frame = match read_next_frame(socket.as_ref().unwrap(), &mut read_buf).await {
            Ok(Some(frame)) => frame,
            Ok(None) => {
                trace!(conn_id, "connection closed by peer");
                close(socket.take().unwrap()).await?;
                worker.close_connection_sessions(conn_id)?;
                break;
            }
            Err(err) => {
                trace!(conn_id, error = %err, "read protocol frame failed");
                let _ = close(socket.take().unwrap()).await;
                return Err(err);
            }
        };

        let request_id = frame.header().request_id();
        trace!(
            conn_id,
            request_id,
            message_type = ?frame.header().message_type(),
            payload_len = frame.header().payload_len(),
            "received protocol frame"
        );
        match dispatch_frame_async(&worker, conn_id, &frame).await {
            Ok(HandleResult::Response(response)) => {
                trace!(
                    conn_id,
                    request_id,
                    response_bytes = response.len(),
                    "dispatch completed with response"
                );
                write_response(socket.as_ref().unwrap(), &response).await?;
            }
            Ok(HandleResult::Transfer(transfer)) => {
                trace!(
                    conn_id,
                    request_id,
                    target_worker = transfer.target_worker(),
                    session_count = transfer.session_ids().len(),
                    "dispatch requested connection transfer"
                );
                let connection = build_transfer(
                    conn_id,
                    remote_addr,
                    socket.take().unwrap(),
                    transfer.clone(),
                );
                WorkerRingLoop::dispatch_mailbox_message(
                    &mailbox_fds,
                    &mailboxes,
                    connection.transfer().target_worker(),
                    WorkerMailboxMsg::AdoptConnection(connection),
                )?;
                break;
            }
            Err(err) => {
                trace!(
                    conn_id,
                    request_id,
                    error = %err,
                    "dispatch returned error response"
                );
                let response = encode_merror_response(request_id, &err)?;
                write_response(socket.as_ref().unwrap(), &response).await?;
            }
        }
        read_buf = frame.into_payload();
    }
    trace!(conn_id, "io_uring connection worker stopped");
    Ok(())
}

fn build_transfer(
    conn_id: u64,
    remote_addr: SocketAddr,
    socket: IoSocket,
    transfer: SessionTransferDispatch,
) -> TransferredConnection {
    TransferredConnection::new(
        ConnectionTransfer::new(
            conn_id,
            transfer.target_worker(),
            crate::server::connection_state::ConnectionState::Active,
            remote_addr,
        ),
        socket.into_raw_fd(),
        transfer.session_ids().to_vec(),
        Some(transfer.action()),
    )
}
