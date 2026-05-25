use crate::env::default_env;
use crate::fd::RawFd;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::sync::mpsc;
use std::time::Duration;

pub fn eventfd() -> RS<RawFd> {
    default_env().sync().eventfd()
}

pub fn notify_eventfd(fd: RawFd) -> RS<()> {
    default_env().sync().notify_eventfd(fd)
}

pub fn read_eventfd(fd: RawFd) -> RS<u64> {
    default_env().sync().read_eventfd(fd)
}

pub fn close_fd(fd: RawFd) -> RS<()> {
    default_env().sync().close_fd(fd)
}

pub struct ChannelSender<T> {
    inner: mpsc::Sender<T>,
}

pub struct ChannelSyncSender<T> {
    inner: mpsc::SyncSender<T>,
}

pub struct ChannelReceiver<T> {
    inner: mpsc::Receiver<T>,
}

impl<T> Clone for ChannelSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Clone for ChannelSyncSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

pub fn channel<T>() -> (ChannelSender<T>, ChannelReceiver<T>) {
    let (tx, rx) = mpsc::channel();
    (ChannelSender { inner: tx }, ChannelReceiver { inner: rx })
}

pub fn sync_channel<T>(bound: usize) -> (ChannelSyncSender<T>, ChannelReceiver<T>) {
    let (tx, rx) = mpsc::sync_channel(bound);
    (
        ChannelSyncSender { inner: tx },
        ChannelReceiver { inner: rx },
    )
}

impl<T> ChannelSender<T> {
    pub fn send(&self, value: T) -> RS<()> {
        match self.inner.send(value) {
            Ok(()) => Ok(()),
            Err(_) => Err(m_error!(EC::SyncErr, "channel send failed")),
        }
    }

    pub fn into_inner(self) -> mpsc::Sender<T> {
        self.inner
    }
}

impl<T> ChannelSyncSender<T> {
    pub fn send(&self, value: T) -> RS<()> {
        match self.inner.send(value) {
            Ok(()) => Ok(()),
            Err(_) => Err(m_error!(EC::SyncErr, "sync_channel send failed")),
        }
    }

    pub fn try_send(&self, value: T) -> RS<()> {
        match self.inner.try_send(value) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(_)) => Err(m_error!(EC::SyncErr, "sync_channel is full")),
            Err(mpsc::TrySendError::Disconnected(_)) => {
                Err(m_error!(EC::SyncErr, "sync_channel is disconnected"))
            }
        }
    }

    pub fn into_inner(self) -> mpsc::SyncSender<T> {
        self.inner
    }
}

impl<T> ChannelReceiver<T> {
    pub fn recv(&self) -> RS<T> {
        self.inner
            .recv()
            .map_err(|e| m_error!(EC::SyncErr, "channel recv failed", e))
    }

    pub fn try_recv(&self) -> RS<Option<T>> {
        match self.inner.try_recv() {
            Ok(v) => Ok(Some(v)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(e) => Err(m_error!(EC::SyncErr, "channel try_recv failed", e)),
        }
    }

    pub fn recv_timeout(&self, dur: Duration) -> RS<Option<T>> {
        match self.inner.recv_timeout(dur) {
            Ok(v) => Ok(Some(v)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(e) => Err(m_error!(EC::SyncErr, "channel recv_timeout failed", e)),
        }
    }

    pub fn into_inner(self) -> mpsc::Receiver<T> {
        self.inner
    }
}
