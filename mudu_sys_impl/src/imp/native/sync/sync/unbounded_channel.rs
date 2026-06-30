use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::sync::mpsc;
use std::time::Duration;

pub struct ChannelSender<T> {
    inner: mpsc::Sender<T>,
}

pub struct SyncReceiver<T> {
    inner: mpsc::Receiver<T>,
}

impl<T> SyncReceiver<T> {
    pub(crate) fn new(inner: mpsc::Receiver<T>) -> Self {
        Self { inner }
    }
}

impl<T> Clone for ChannelSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

pub fn unbounded_channel<T>() -> (ChannelSender<T>, SyncReceiver<T>) {
    let (tx, rx) = mpsc::channel();
    (ChannelSender { inner: tx }, SyncReceiver { inner: rx })
}

impl<T> ChannelSender<T> {
    pub fn send(&self, value: T) -> RS<()> {
        match self.inner.send(value) {
            Ok(()) => Ok(()),
            Err(_) => Err(mudu_error!(ErrorCode::ChannelClosed, "channel send failed")),
        }
    }
}

impl<T> SyncReceiver<T> {
    pub fn recv(&self) -> RS<T> {
        self.inner
            .recv()
            .map_err(|e| mudu_error!(ErrorCode::ChannelClosed, "channel recv failed", e))
    }

    pub fn try_recv(&self) -> RS<Option<T>> {
        match self.inner.try_recv() {
            Ok(v) => Ok(Some(v)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(e) => Err(mudu_error!(
                ErrorCode::ChannelClosed,
                "channel try_recv failed",
                e
            )),
        }
    }

    pub fn recv_timeout(&self, dur: Duration) -> RS<Option<T>> {
        match self.inner.recv_timeout(dur) {
            Ok(v) => Ok(Some(v)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(e) => Err(mudu_error!(
                ErrorCode::ChannelClosed,
                "channel recv_timeout failed",
                e
            )),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{unbounded_channel, ChannelSender, SyncReceiver};
    use mudu::error::ErrorCode;

    #[test]
    fn unbounded_channel_send_recv() {
        let (tx, rx): (ChannelSender<i32>, SyncReceiver<i32>) = unbounded_channel();
        tx.send(42).unwrap();
        tx.send(43).unwrap();
        assert_eq!(rx.recv().unwrap(), 42);
        assert_eq!(rx.recv().unwrap(), 43);
    }

    #[test]
    fn recv_after_disconnect() {
        let (tx, rx): (ChannelSender<i32>, SyncReceiver<i32>) = unbounded_channel();
        tx.send(1).unwrap();
        drop(tx);
        assert_eq!(rx.recv().unwrap(), 1);
        let err = rx.recv().unwrap_err();
        assert_eq!(err.ec(), ErrorCode::ChannelClosed);
    }
}
