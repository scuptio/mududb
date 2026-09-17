use tokio::sync::watch;

#[derive(Clone)]
pub struct StopTx {
    inner: watch::Sender<bool>,
}

#[derive(Clone)]
pub struct StopRx {
    inner: watch::Receiver<bool>,
}

pub fn stop_channel() -> (StopTx, StopRx) {
    let (tx, rx) = watch::channel(false);
    (StopTx { inner: tx }, StopRx { inner: rx })
}

impl StopTx {
    pub fn stop(&self) {
        let _ = self.inner.send(true);
    }
}

impl StopRx {
    pub fn is_stopped(&self) -> bool {
        *self.inner.borrow()
    }

    pub async fn changed(&mut self) -> bool {
        self.inner.changed().await.is_ok()
    }
}
