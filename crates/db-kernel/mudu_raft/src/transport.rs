//! Transport abstraction for outbound raft messages.
//!
//! The core and the event loop never perform network I/O; outbound messages
//! go through [`RaftTransport`]. [`InMemoryNetwork`]/[`InMemoryTransport`]
//! implement the trait over in-process channels so unit tests can run whole
//! clusters deterministically in one process, including partitions.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use mudu::common::result::RS;
use mudu_sys::sync::sync_::unbounded_channel::ChannelSender;
use mudu_sys::sync::SMutex;

use crate::core::conf::NID;
use crate::core::message::RaftMsg;
use crate::node::RaftIn;

/// Outbound message transport.
///
/// Implementations must be non-blocking: a slow or partitioned peer must not
/// stall the raft event loop. Delivery is best-effort (raft tolerates loss).
pub trait RaftTransport: Send + Sync {
    /// Sends `msg` to node `to`.
    fn send(&self, to: NID, msg: RaftMsg) -> RS<()>;
}

/// An in-process raft network for tests: a registry mapping node id to the
/// node's input channel, plus a drop set simulating partitions.
#[derive(Clone, Default)]
pub struct InMemoryNetwork {
    inner: Arc<InMemoryNetworkInner>,
}

#[derive(Default)]
struct InMemoryNetworkInner {
    nodes: SMutex<HashMap<NID, ChannelSender<RaftIn>>>,
    dropped_links: SMutex<HashSet<(NID, NID)>>,
}

impl InMemoryNetwork {
    /// Creates an empty network.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a node's input channel (called when a node starts).
    pub fn register(&self, node_id: NID, tx: ChannelSender<RaftIn>) -> RS<()> {
        self.inner.nodes.lock()?.insert(node_id, tx);
        Ok(())
    }

    /// Removes a node's registration.
    pub fn unregister(&self, node_id: NID) -> RS<()> {
        self.inner.nodes.lock()?.remove(&node_id);
        Ok(())
    }

    /// Creates a transport endpoint for `node_id` on this network.
    pub fn transport(&self, node_id: NID) -> Arc<InMemoryTransport> {
        Arc::new(InMemoryTransport {
            node_id,
            network: self.clone(),
        })
    }

    /// Drops all messages sent `from` -> `to` until healed.
    pub fn drop_link(&self, from: NID, to: NID) -> RS<()> {
        self.inner.dropped_links.lock()?.insert((from, to));
        Ok(())
    }

    /// Heals one directed link.
    pub fn heal_link(&self, from: NID, to: NID) -> RS<()> {
        self.inner.dropped_links.lock()?.remove(&(from, to));
        Ok(())
    }

    /// Drops all links to and from `node_id`.
    pub fn isolate(&self, node_id: NID) -> RS<()> {
        let nodes: Vec<NID> = self.inner.nodes.lock()?.keys().cloned().collect();
        let mut drops = self.inner.dropped_links.lock()?;
        for n in nodes {
            if n != node_id {
                drops.insert((node_id, n));
                drops.insert((n, node_id));
            }
        }
        Ok(())
    }

    /// Heals all links.
    pub fn heal_all(&self) -> RS<()> {
        self.inner.dropped_links.lock()?.clear();
        Ok(())
    }

    fn is_dropped(&self, from: NID, to: NID) -> bool {
        match self.inner.dropped_links.lock() {
            Ok(drops) => drops.contains(&(from, to)),
            Err(_) => false,
        }
    }

    fn lookup(&self, node_id: NID) -> Option<ChannelSender<RaftIn>> {
        match self.inner.nodes.lock() {
            Ok(nodes) => nodes.get(&node_id).cloned(),
            Err(_) => None,
        }
    }
}

/// A transport endpoint of an [`InMemoryNetwork`].
pub struct InMemoryTransport {
    node_id: NID,
    network: InMemoryNetwork,
}

impl RaftTransport for InMemoryTransport {
    fn send(&self, to: NID, msg: RaftMsg) -> RS<()> {
        // delivery is best-effort: unknown, stopped or partitioned peers
        // silently swallow the message, as a real network would
        if self.network.is_dropped(self.node_id, to) {
            return Ok(());
        }
        if let Some(tx) = self.network.lookup(to) {
            // a closed channel means the peer is gone; not an error
            let _ = tx.send(RaftIn::Net {
                from: self.node_id,
                msg,
            });
        }
        Ok(())
    }
}
