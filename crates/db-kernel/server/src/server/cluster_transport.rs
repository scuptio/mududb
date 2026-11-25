//! The cluster data-plane transport: one TCP connection per peer node
//! multiplexing channel envelopes between workers of different nodes.
//!
//! Worker ids are derived from `(node_id, worker_index)`, so outbound routing
//! is pure computation over the static topology (`ClusterDirectory`); the
//! handshake only fences the topology version. Connections are
//! unidirectional in effect (each node dials every peer proactively); the
//! accept side reads frames and delivers envelopes into the local worker
//! mailboxes or the channel registry.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use mudu::channel::Envelope;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::net::sync::StdTcpListener;
use mudu_sys::net::AsyncTcpStream;
use mudu_sys::sync::async_::stop_flag::StopRx;
use mudu_sys::sync::SMutex;
use mudu_sys::task::sync::SJoinHandle;
use mudu_sys::tokio::io::{AsyncReadExt, AsyncWriteExt};
mod queue;
use mudu_utils::channel::channel_now_ms;
use mudu_utils::task_async::{spawn_local_detached, CurrentThreadTaskRuntime};
use queue::{peer_queue, PeerQueue, PeerReceiver};
use serde::{Deserialize, Serialize};
use tracing::{debug, trace};

use super::cluster_directory::ClusterDirectory;
use worker::worker::adopt_worker_listener;

/// Reconnect backoff of a peer connector after a failed attempt.
const CLUSTER_RECONNECT_BACKOFF: Duration = Duration::from_millis(200);
/// Largest accepted cluster frame (a partition RPC write set is bounded by
/// the worker log batch, far below this cap).
const CLUSTER_FRAME_MAX_BYTES: u32 = 64 * 1024 * 1024;

/// Wire frame of the cluster channel: 4-byte little-endian length prefix
/// followed by the rmp-serde encoded frame.
#[derive(Serialize, Deserialize, Debug)]
enum ClusterFrame {
    /// First frame on every outbound connection; the accept side closes the
    /// connection on a topology-version mismatch.
    Hello { node_id: u64, topology_version: u64 },
    /// A channel envelope addressed to a worker (or registry endpoint) of
    /// the receiving node.
    Envelope(Envelope),
}

/// Delivery callback of inbound envelopes: routes to the addressed worker's
/// mailbox or the channel registry (built in `serve`).
pub type ClusterDeliver = Arc<dyn Fn(Envelope) -> RS<()> + Send + Sync>;

/// The cluster transport of one node: outbound peer queues plus the
/// listener/connector driver.
pub struct ClusterTransport {
    directory: Arc<ClusterDirectory>,
    /// peer node_id -> outbound frame queue; filled when `start` runs.
    peers: SMutex<HashMap<u64, PeerQueue>>,
    deliver: ClusterDeliver,
    stopped: AtomicBool,
}

impl ClusterTransport {
    /// Creates the transport; no socket is touched until [`Self::start`].
    pub fn new(directory: Arc<ClusterDirectory>, deliver: ClusterDeliver) -> Arc<Self> {
        Arc::new(Self {
            directory,
            peers: SMutex::new(HashMap::new()),
            deliver,
            stopped: AtomicBool::new(false),
        })
    }

    /// The cluster directory this transport routes with.
    pub(crate) fn directory(&self) -> &Arc<ClusterDirectory> {
        &self.directory
    }

    /// Queues `envelope` for the node hosting its destination worker.
    /// Non-blocking; expired envelopes are dropped. Delivery is
    /// at-most-once, matching the channel semantics contract.
    pub fn send_envelope(&self, envelope: Envelope) -> RS<()> {
        if self.stopped.load(Ordering::Acquire) {
            return Err(mudu_error!(
                ErrorCode::ChannelClosed,
                "cluster transport stopped"
            ));
        }
        if envelope.is_expired_at(channel_now_ms()) {
            return Err(mudu_error!(
                ErrorCode::TimedOut,
                "cluster envelope deadline expired before send"
            ));
        }
        let dst = *envelope.dst();
        let Some(node_id) = self.directory.node_of_worker(dst) else {
            return Err(mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("no cluster node hosts worker {}", dst)
            ));
        };
        if node_id == self.directory.self_node_id() {
            // Same-node delivery (the worker channel's mailbox route normally
            // catches this first; this is the defensive fallback).
            return (self.deliver)(envelope);
        }
        let sender = self
            .peers
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "cluster transport lock poisoned"))?
            .get(&node_id)
            .cloned();
        match sender {
            Some(sender) => sender.send(envelope),
            None => Err(mudu_error!(
                ErrorCode::Network,
                format!("cluster peer {} is not connected yet", node_id)
            )),
        }
    }

    /// Starts the transport thread: the accept loop on the pre-bound
    /// listener plus one connector task per peer node. Returns the thread
    /// handle; stopping happens via `stop_rx` and [`Self::stop`].
    pub fn start(
        self: &Arc<Self>,
        listener: StdTcpListener,
        stop_rx: StopRx,
    ) -> RS<SJoinHandle<RS<()>>> {
        let transport = self.clone();
        mudu_sys::task::sync::spawn_thread_named("cluster-transport".to_string(), move || {
            let runtime = CurrentThreadTaskRuntime::new().map_err(|e| {
                mudu_error!(ErrorCode::Tokio, "build cluster transport runtime error", e)
            })?;
            runtime.block_on(async move { transport.run(listener, stop_rx).await })
        })
    }

    /// Stops the transport: connectors and the accept loop exit on their
    /// stop channels; queued frames are dropped.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
    }

    async fn run(self: Arc<Self>, listener: StdTcpListener, stop_rx: StopRx) -> RS<()> {
        listener.set_nonblocking(true).map_err(|e| {
            mudu_error!(
                ErrorCode::Network,
                "set cluster listener nonblocking error",
                e
            )
        })?;
        let listener = adopt_worker_listener(listener).await?;
        trace!(
            port = ?listener.local_addr(),
            "cluster channel listener ready"
        );
        // Proactive connectors: every peer node gets a queue and a
        // reconnecting connector task. The full mesh covers the data group
        // members and the dedicated control nodes.
        for (peer_node_id, host, channel_port) in self.directory.topology().all_peer_nodes() {
            if peer_node_id == self.directory.self_node_id() {
                continue;
            }
            let (tx, rx) = peer_queue();
            self.peers
                .lock()
                .map_err(|_| mudu_error!(ErrorCode::Internal, "cluster transport lock poisoned"))?
                .insert(peer_node_id, tx);
            let connector = self.clone();
            let peer_stop_rx = stop_rx.clone();
            let addr = format!("{host}:{channel_port}");
            let _ = spawn_local_detached("cluster_connector", async move {
                connector
                    .run_connector(peer_node_id, addr, rx, peer_stop_rx)
                    .await;
                Ok::<(), mudu::error::MuduError>(())
            });
        }
        // Accept loop.
        let mut stop_rx = stop_rx;
        loop {
            mudu_sys::tokio::select! {
                accept_result = listener.accept() => {
                    let (stream, remote_addr) = accept_result.map_err(|e| {
                        mudu_error!(ErrorCode::Network, "accept cluster connection error", e)
                    })?;
                    trace!(remote = %remote_addr, "accepted cluster connection");
                    let handler = self.clone();
                    let conn_stop_rx = stop_rx.clone();
                    let _ = spawn_local_detached("cluster_inbound", async move {
                        if let Err(err) = handler.run_inbound(stream, conn_stop_rx).await {
                            debug!(remote = %remote_addr, "cluster inbound connection closed: {}", err);
                        }
                        Ok::<(), mudu::error::MuduError>(())
                    });
                }
                changed = stop_rx.changed() => {
                    if !changed || stop_rx.is_stopped() {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Runs one outbound connector: reconnect loop of connect → hello →
    /// drain the outbound queue, with backoff between attempts.
    async fn run_connector(
        self: Arc<Self>,
        peer_node_id: u64,
        addr: String,
        mut rx: PeerReceiver,
        mut stop_rx: StopRx,
    ) {
        loop {
            if self.stopped.load(Ordering::Relaxed) || stop_rx.is_stopped() {
                return;
            }
            match AsyncTcpStream::connect(addr.clone()).await {
                Ok(stream) => {
                    if let Err(err) = self.run_peer_session(stream, &mut rx, &mut stop_rx).await {
                        debug!(
                            peer_node_id,
                            addr, "cluster peer connection closed: {}", err
                        );
                    }
                }
                Err(err) => {
                    trace!(peer_node_id, addr, "cluster connect failed: {}", err);
                }
            }
            mudu_sys::tokio::select! {
                _ = mudu_sys::task::async_::sleep(CLUSTER_RECONNECT_BACKOFF) => {}
                changed = stop_rx.changed() => {
                    if !changed || stop_rx.is_stopped() {
                        return;
                    }
                }
            }
        }
    }

    /// One established outbound session: send our hello, then drain the
    /// outbound queue until an error or stop.
    async fn run_connector_session(
        &self,
        stream: &mut AsyncTcpStream,
        rx: &mut PeerReceiver,
        stop_rx: &mut StopRx,
    ) -> RS<()> {
        write_frame(
            stream,
            &ClusterFrame::Hello {
                node_id: self.directory.self_node_id(),
                topology_version: self.directory.topology().version,
            },
        )
        .await?;
        // The accept side verifies our hello; nothing to read here.
        loop {
            mudu_sys::tokio::select! {
                frame = rx.recv() => {
                    let Some(frame) = frame else {
                        return Ok(());
                    };
                    if frame.deadline.is_some_and(|deadline| channel_now_ms() >= deadline) {
                        continue;
                    }
                    mudu_sys::tokio::select! {
                        result = write_payload(stream, &frame.bytes) => result?,
                        _ = stop_rx.changed() => return Ok(()),
                    }
                }
                changed = stop_rx.changed() => {
                    if !changed || stop_rx.is_stopped() {
                        return Ok(());
                    }
                }
            }
        }
    }

    async fn run_peer_session(
        &self,
        stream: AsyncTcpStream,
        rx: &mut PeerReceiver,
        stop_rx: &mut StopRx,
    ) -> RS<()> {
        let mut stream = stream;
        stream
            .set_nodelay(true)
            .map_err(|e| mudu_error!(ErrorCode::Network, "set cluster nodelay error", e))?;
        self.run_connector_session(&mut stream, rx, stop_rx).await
    }

    /// Runs one accepted connection: read and verify the hello, then deliver
    /// envelopes until close.
    async fn run_inbound(&self, stream: AsyncTcpStream, mut stop_rx: StopRx) -> RS<()> {
        let mut stream = stream;
        stream
            .set_nodelay(true)
            .map_err(|e| mudu_error!(ErrorCode::Network, "set cluster nodelay error", e))?;
        let hello = read_frame(&mut stream, &mut stop_rx).await?;
        match hello {
            ClusterFrame::Hello {
                node_id,
                topology_version,
            } => {
                if topology_version != self.directory.topology().version {
                    return Err(mudu_error!(
                        ErrorCode::InvalidState,
                        format!(
                            "cluster topology version mismatch: peer {} runs {}, we run {}",
                            node_id,
                            topology_version,
                            self.directory.topology().version
                        )
                    ));
                }
                trace!(node_id, "cluster peer hello accepted");
            }
            _ => {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    "cluster connection did not start with a hello frame"
                ));
            }
        }
        loop {
            let frame = read_frame(&mut stream, &mut stop_rx).await?;
            match frame {
                ClusterFrame::Envelope(envelope) => {
                    if envelope.is_expired_at(channel_now_ms()) {
                        continue;
                    }
                    (self.deliver)(envelope)?;
                }
                ClusterFrame::Hello { .. } => {
                    return Err(mudu_error!(
                        ErrorCode::InvalidState,
                        "duplicate cluster hello frame"
                    ));
                }
            }
        }
    }
}

async fn write_frame(stream: &mut AsyncTcpStream, frame: &ClusterFrame) -> RS<()> {
    let payload = rmp_serde::to_vec(frame)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode cluster frame error", e))?;
    write_payload(stream, &payload).await
}

async fn write_payload(stream: &mut AsyncTcpStream, payload: &[u8]) -> RS<()> {
    let len = u32::try_from(payload.len())
        .map_err(|_| mudu_error!(ErrorCode::InvalidArgument, "cluster frame too large"))?;
    stream
        .write_all(&len.to_le_bytes())
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, "write cluster frame length error", e))?;
    stream
        .write_all(payload)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, "write cluster frame payload error", e))?;
    Ok(())
}

async fn read_frame(stream: &mut AsyncTcpStream, stop_rx: &mut StopRx) -> RS<ClusterFrame> {
    let mut len_buf = [0u8; 4];
    mudu_sys::tokio::select! {
        read = stream.read_exact(&mut len_buf) => {
            read.map_err(|e| mudu_error!(ErrorCode::Network, "read cluster frame length error", e))?;
        }
        changed = stop_rx.changed() => {
            if !changed || stop_rx.is_stopped() {
                return Err(mudu_error!(ErrorCode::ChannelClosed, "cluster transport stopped"));
            }
        }
    }
    let len = u32::from_le_bytes(len_buf);
    if len > CLUSTER_FRAME_MAX_BYTES {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("cluster frame {} bytes exceeds the cap", len)
        ));
    }
    let mut payload = vec![0u8; len as usize];
    mudu_sys::tokio::select! {
        read = stream.read_exact(&mut payload) => {
            read.map_err(|e| mudu_error!(ErrorCode::Network, "read cluster frame payload error", e))?;
        }
        changed = stop_rx.changed() => {
            if !changed || stop_rx.is_stopped() {
                return Err(mudu_error!(ErrorCode::ChannelClosed, "cluster transport stopped"));
            }
        }
    }
    rmp_serde::from_slice(&payload)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode cluster frame error", e))
}

impl Drop for ClusterTransport {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use ::catalog::contract::cluster_topology::{
        ClusterGroup, ClusterGroupMember, ClusterTopology,
    };
    use ::catalog::contract::derive_oid::derive_worker_oid;
    use mudu::channel::{DeliveryMode, MessageKind};
    use mudu_sys::net::sync::bind_tcp;
    use mudu_sys::sync::async_::stop_flag::stop_channel;

    /// Builds a two-node topology with the two given channel ports.
    fn two_node_topology(port1: u16, port2: u16) -> ClusterTopology {
        ClusterTopology {
            version: 3,
            group: vec![
                ClusterGroup {
                    group_id: 1,
                    members: vec![ClusterGroupMember {
                        node_id: 1,
                        host: "127.0.0.1".to_string(),
                        channel_port: port1,
                        client_port: 15432,
                    }],
                    worker_count: 1,
                },
                ClusterGroup {
                    group_id: 2,
                    members: vec![ClusterGroupMember {
                        node_id: 2,
                        host: "127.0.0.1".to_string(),
                        channel_port: port2,
                        client_port: 15433,
                    }],
                    worker_count: 1,
                },
            ],
            placements: vec![],
            control: None,
            cluster_id: 0,
        }
    }

    fn bound_listener() -> (StdTcpListener, u16) {
        let listener = bind_tcp("127.0.0.1:0".parse().unwrap()).unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    #[test]
    fn transport_delivers_envelope_between_two_nodes() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let (listener1, port1) = bound_listener();
            let (listener2, port2) = bound_listener();
            let topology = two_node_topology(port1, port2);

            let (tx2, rx2) = std::sync::mpsc::channel::<Envelope>();
            let deliver2: ClusterDeliver = Arc::new(move |envelope| {
                tx2.send(envelope).map_err(|_| {
                    mudu_error!(ErrorCode::ChannelClosed, "test delivery queue closed")
                })?;
                Ok(())
            });
            let directory2 = ClusterDirectory::new(topology.clone(), 2).unwrap();
            let transport2 = ClusterTransport::new(directory2, deliver2);
            let (stop2_tx, stop2_rx) = stop_channel();
            let handle2 = transport2.start(listener2, stop2_rx).unwrap();

            let deliver1: ClusterDeliver = Arc::new(|_envelope| {
                Err(mudu_error!(
                    ErrorCode::InvalidState,
                    "node 1 receives nothing"
                ))
            });
            let directory1 = ClusterDirectory::new(topology, 1).unwrap();
            let transport1 = ClusterTransport::new(directory1, deliver1);
            let (stop1_tx, stop1_rx) = stop_channel();
            let handle1 = transport1.start(listener1, stop1_rx).unwrap();

            // Wait for node 1's connector to establish the session.
            let target = derive_worker_oid(2, 0);
            let mut delivered = None;
            for _ in 0..50 {
                let envelope = Envelope::new(
                    1,
                    None,
                    derive_worker_oid(1, 0),
                    target,
                    MessageKind::User(7),
                    b"ping".to_vec(),
                    DeliveryMode::Request,
                );
                match transport1.send_envelope(envelope) {
                    Ok(()) => {}
                    Err(_) => {
                        mudu_sys::task::async_::sleep(Duration::from_millis(20))
                            .await
                            .unwrap();
                        continue;
                    }
                }
                match rx2.recv_timeout(Duration::from_millis(500)) {
                    Ok(envelope) => {
                        delivered = Some(envelope);
                        break;
                    }
                    Err(_) => continue,
                }
            }
            let envelope = delivered.expect("envelope delivered to node 2");
            assert_eq!(envelope.payload(), b"ping");
            assert_eq!(*envelope.dst(), target);

            transport1.stop();
            transport2.stop();
            stop1_tx.stop();
            stop2_tx.stop();
            let _ = handle1.join().unwrap();
            let _ = handle2.join().unwrap();
            Ok::<(), mudu::error::MuduError>(())
        })
        .unwrap()
        .unwrap();
    }
}
