//! WebSocket session registry: tracks connected clients by zone and fans out
//! pushed messages to their outbound channels.

use mudu::common::result::RS;
use mudu_sys::sync::SMutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

/// A session identifier minted on connection.
pub type SessionId = u64;

/// A connected client: its zone and the channel the session loop reads from.
struct SessionEntry {
    zone: String,
    sender: mpsc::Sender<String>,
}

/// Thread-safe registry of connected WebSocket sessions, keyed by session id.
///
/// Each session owns a bounded outbound channel; the session loop drains it
/// and writes frames to the socket. Pushes are fanned out per zone so a
/// broadcast reaches only the clients subscribed to that zone.
pub struct SessionRegistry {
    inner: SMutex<HashMap<SessionId, SessionEntry>>,
    next_id: AtomicU64,
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self {
            inner: SMutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }
}

impl SessionRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a session in `zone`, returning its minted id.
    pub fn register(&self, zone: String, sender: mpsc::Sender<String>) -> SessionId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let entry = SessionEntry { zone, sender };
        self.inner
            .lock()
            .expect("session registry poisoned")
            .insert(id, entry);
        id
    }

    /// Removes a session; its outbound channel is dropped and the session
    /// loop exits.
    pub fn unregister(&self, id: SessionId) {
        self.inner
            .lock()
            .expect("session registry poisoned")
            .remove(&id);
    }

    /// Pushes `text` to every session in `zone`, returning how many received
    /// it. Sends happen outside the lock so a slow receiver cannot block the
    /// registry.
    pub fn broadcast(&self, zone: &str, text: String) -> usize {
        let senders: Vec<mpsc::Sender<String>> = {
            let guard = self.inner.lock().expect("session registry poisoned");
            guard
                .values()
                .filter(|entry| entry.zone == zone)
                .map(|entry| entry.sender.clone())
                .collect()
        };
        let mut delivered = 0;
        for sender in senders {
            if sender.try_send(text.clone()).is_ok() {
                delivered += 1;
            }
        }
        delivered
    }

    /// Returns the number of connected sessions in `zone`.
    pub fn count(&self, zone: &str) -> RS<usize> {
        let guard = self.inner.lock()?;
        Ok(guard.values().filter(|entry| entry.zone == zone).count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel() -> (mpsc::Sender<String>, mpsc::Receiver<String>) {
        mpsc::channel(4)
    }

    #[test]
    fn broadcast_reaches_only_matching_zone() {
        let registry = SessionRegistry::new();
        let (tx_a, mut rx_a) = channel();
        let (tx_b, _rx_b) = channel();
        registry.register("zone-a".to_string(), tx_a);
        registry.register("zone-b".to_string(), tx_b);

        assert_eq!(registry.broadcast("zone-a", "hello-a".to_string()), 1);
        assert_eq!(registry.broadcast("zone-b", "hello-b".to_string()), 1);
        assert_eq!(rx_a.try_recv().unwrap(), "hello-a");
    }

    #[test]
    fn unregister_stops_broadcast() {
        let registry = SessionRegistry::new();
        let (tx, mut rx) = channel();
        let id = registry.register("z".to_string(), tx);
        assert_eq!(registry.count("z").unwrap(), 1);
        registry.unregister(id);
        assert_eq!(registry.broadcast("z", "gone".to_string()), 0);
        assert!(rx.try_recv().is_err());
    }
}
