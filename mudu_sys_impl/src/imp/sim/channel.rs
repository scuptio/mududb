use crate::sync::blocking::{ChannelReceiver, ChannelSender, ChannelSyncSender};
use crate::sync::blocking::{channel as blocking_channel, sync_channel as blocking_sync_channel};

pub fn channel<T>() -> (ChannelSender<T>, ChannelReceiver<T>) {
    blocking_channel()
}

pub fn sync_channel<T>(bound: usize) -> (ChannelSyncSender<T>, ChannelReceiver<T>) {
    blocking_sync_channel(bound)
}
