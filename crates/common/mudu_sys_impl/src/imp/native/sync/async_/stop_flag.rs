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
