use tokio::sync::watch;

/// Sending half of a stop-signal channel.
#[derive(Clone)]
pub struct StopTx {
    inner: watch::Sender<bool>,
}

/// Receiving half of a stop-signal channel.
#[derive(Clone)]
pub struct StopRx {
    inner: watch::Receiver<bool>,
}

/// Creates a stop-signal channel: [`StopTx::stop`] flips the shared flag
/// observed by all receivers.
pub fn stop_channel() -> (StopTx, StopRx) {
    let (tx, rx) = watch::channel(false);
    (StopTx { inner: tx }, StopRx { inner: rx })
}

impl StopTx {
    /// Signals stop to all receivers.
    pub fn stop(&self) {
        let _ = self.inner.send(true);
    }
}

impl StopRx {
    /// Returns `true` if stop has been signaled.
    pub fn is_stopped(&self) -> bool {
        *self.inner.borrow()
    }

    /// Waits for a change of the stop flag; returns `false` if all senders
    /// were dropped instead.
    pub async fn changed(&mut self) -> bool {
        self.inner.changed().await.is_ok()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::stop_channel;

    #[test]
    fn stop_flips_the_flag_for_all_receivers() {
        let (tx, rx) = stop_channel();
        let rx_clone = rx.clone();
        assert!(!rx.is_stopped());
        assert!(!rx_clone.is_stopped());
        tx.stop();
        assert!(rx.is_stopped());
        assert!(rx_clone.is_stopped());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn changed_observes_stop_and_sender_drop() {
        let (tx, mut rx) = stop_channel();
        tx.stop();
        assert!(rx.changed().await);

        let (tx, mut rx) = stop_channel();
        drop(tx);
        assert!(!rx.changed().await);
    }
}
