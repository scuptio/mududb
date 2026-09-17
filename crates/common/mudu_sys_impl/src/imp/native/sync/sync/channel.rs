pub(crate) use super::unbounded_channel::SyncReceiver;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::sync::mpsc;

pub struct SyncSender<T> {
    inner: mpsc::SyncSender<T>,
}

impl<T> Clone for SyncSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

pub fn sync_channel<T>(bound: usize) -> (SyncSender<T>, SyncReceiver<T>) {
    let (tx, rx) = mpsc::sync_channel(bound);
    (SyncSender { inner: tx }, SyncReceiver::new(rx))
}

impl<T> SyncSender<T> {
    pub fn send(&self, value: T) -> RS<()> {
        match self.inner.send(value) {
            Ok(()) => Ok(()),
            Err(_) => Err(mudu_error!(
                ErrorCode::ChannelClosed,
                "sync_channel send failed"
            )),
        }
    }

    pub fn try_send(&self, value: T) -> RS<()> {
        match self.inner.try_send(value) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(_)) => Err(mudu_error!(
                ErrorCode::Synchronization,
                "sync_channel is full"
            )),
            Err(mpsc::TrySendError::Disconnected(_)) => Err(mudu_error!(
                ErrorCode::ChannelClosed,
                "sync_channel is disconnected"
            )),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{sync_channel, SyncReceiver, SyncSender};
    use mudu::error::ErrorCode;

    #[test]
    fn sync_channel_returns_sender_receiver() {
        let (tx, rx): (SyncSender<i32>, SyncReceiver<i32>) = sync_channel(1);
        tx.send(42).unwrap();
        assert_eq!(rx.recv().unwrap(), 42);
    }

    #[test]
    fn sync_channel_send_success() {
        let (tx, _rx): (SyncSender<i32>, SyncReceiver<i32>) = sync_channel(2);
        tx.send(1).unwrap();
        tx.send(2).unwrap();
    }

    #[test]
    fn sync_channel_send_closed_after_receiver_drop() {
        let (tx, rx): (SyncSender<i32>, SyncReceiver<i32>) = sync_channel(1);
        drop(rx);
        let err = tx.send(1).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::ChannelClosed);
    }

    #[test]
    fn sync_channel_try_send_success() {
        let (tx, _rx): (SyncSender<i32>, SyncReceiver<i32>) = sync_channel(2);
        tx.try_send(1).unwrap();
        tx.try_send(2).unwrap();
    }

    #[test]
    fn sync_channel_try_send_full() {
        let (tx, rx): (SyncSender<i32>, SyncReceiver<i32>) = sync_channel(1);
        tx.try_send(1).unwrap();
        let err = tx.try_send(2).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Synchronization);
        drop(rx);
    }

    #[test]
    fn sync_channel_try_send_closed() {
        let (tx, rx): (SyncSender<i32>, SyncReceiver<i32>) = sync_channel(1);
        drop(rx);
        let err = tx.try_send(1).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::ChannelClosed);
    }
}
