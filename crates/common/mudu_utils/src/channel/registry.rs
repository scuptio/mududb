//! The endpoint registry and lifecycle: [`spawn_endpoint`] registers a
//! transport under an endpoint id and hands back a typed
//! [`Channel`] plus an [`EndpointGuard`] owning the registration.

use super::endpoint::ChannelEndpointRef;
use super::typed::Channel;
use mudu::channel::{EndpointId, ServerInstanceId};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::sync::OnceLock;

thread_local! {
    static CURRENT_CHANNEL: UnsafeCell<Option<ChannelEndpointRef>> =
        const { UnsafeCell::new(None) };
}

fn channel_registry() -> &'static SMutex<HashMap<(ServerInstanceId, OID), ChannelEndpointRef>> {
    static REGISTRY: OnceLock<SMutex<HashMap<(ServerInstanceId, OID), ChannelEndpointRef>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| SMutex::new(HashMap::new()))
}

/// Milliseconds since the Unix epoch on the channel clock: the time base of
/// `deadline_ms` fields and waiter sweeping.
pub fn channel_now_ms() -> u64 {
    mudu_sys::time::system_time_now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

/// Generates a fresh idempotency key for a logical channel operation.
pub fn gen_request_id() -> mudu::channel::RequestId {
    mudu_sys::random::uuid_v4().as_u128()
}

/// Registers `channel` as endpoint `endpoint_id` under the given server
/// instance. Any thread may register an endpoint; database workers register
/// their worker id, other threads register an OID allocated for them (see
/// [`crate::oid::gen_oid`]).
///
/// Prefer [`spawn_endpoint`], which couples registration to an
/// [`EndpointGuard`], over calling this directly.
pub fn register_channel(
    server_instance_id: ServerInstanceId,
    endpoint_id: EndpointId,
    channel: &ChannelEndpointRef,
) -> RS<()> {
    let mut registry = channel_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "channel registry lock poisoned"))?;
    registry.insert((server_instance_id, endpoint_id), channel.clone());
    Ok(())
}

/// Removes the registration of endpoint `endpoint_id`; a no-op when none is
/// registered.
///
/// Prefer dropping an [`EndpointGuard`] over calling this directly.
pub fn unregister_channel(server_instance_id: ServerInstanceId, endpoint_id: EndpointId) -> RS<()> {
    let mut registry = channel_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "channel registry lock poisoned"))?;
    let Some(_channel) = registry.remove(&(server_instance_id, endpoint_id)) else {
        return Ok(());
    };
    Ok(())
}

/// Looks up the channel registered for endpoint `endpoint_id` of the given
/// server instance; errors when none is registered.
pub fn channel_for(
    server_instance_id: ServerInstanceId,
    endpoint_id: EndpointId,
) -> RS<ChannelEndpointRef> {
    let registry = channel_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "channel registry lock poisoned"))?;
    registry
        .get(&(server_instance_id, endpoint_id))
        .cloned()
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!(
                    "channel for server {} endpoint {} is not registered",
                    server_instance_id, endpoint_id
                )
            )
        })
}

/// Owns the registration of an endpoint: dropping the guard unregisters the
/// endpoint from the registry and closes the transport (pending receivers
/// observe `ChannelClosed`, buffered state is drained).
pub struct EndpointGuard {
    server_instance_id: ServerInstanceId,
    endpoint_id: EndpointId,
    channel: ChannelEndpointRef,
}

impl EndpointGuard {
    /// The endpoint id this guard keeps registered.
    pub fn endpoint_id(&self) -> EndpointId {
        self.endpoint_id
    }
}

impl Drop for EndpointGuard {
    fn drop(&mut self) {
        let _ = unregister_channel(self.server_instance_id, self.endpoint_id);
        self.channel.close();
    }
}

/// Registers `channel` as endpoint `endpoint_id` under the given server
/// instance and returns the typed [`Channel`] view plus the guard owning
/// the registration. This is the one call both database workers and
/// standalone threads use to join the channel.
pub fn spawn_endpoint(
    server_instance_id: ServerInstanceId,
    endpoint_id: EndpointId,
    channel: &ChannelEndpointRef,
) -> RS<(Channel, EndpointGuard)> {
    register_channel(server_instance_id, endpoint_id, channel)?;
    Ok((
        Channel::new(channel.clone()),
        EndpointGuard {
            server_instance_id,
            endpoint_id,
            channel: channel.clone(),
        },
    ))
}

/// Registers `channel` as the endpoint of the *current thread*, using the
/// thread's task id (its identity OID) as the endpoint id — the channel
/// convention that an endpoint's id is the OID of the thread hosting it.
/// Database workers instead register their worker id (which is also their
/// thread's task id). Fails with `InvalidState` when the current thread was
/// not spawned through `mudu_sys` and therefore has no thread OID.
pub fn spawn_endpoint_for_current_thread(
    server_instance_id: ServerInstanceId,
    channel: &ChannelEndpointRef,
) -> RS<(Channel, EndpointGuard)> {
    let endpoint_id = mudu_sys::task::sync::try_this_thread_task_id().ok_or_else(|| {
        mudu_error!(
            ErrorCode::InvalidState,
            "current thread has no OID: not spawned through mudu_sys"
        )
    })?;
    spawn_endpoint(server_instance_id, endpoint_id, channel)
}

/// Installs `channel` as the current thread's channel.
pub fn set_current_channel(channel: ChannelEndpointRef) {
    CURRENT_CHANNEL.with(|slot| {
        // Safety: the slot is thread-local and only mutated through these helpers.
        unsafe {
            *slot.get() = Some(channel);
        }
    });
}

/// Removes the current thread's channel.
pub fn unset_current_channel() {
    CURRENT_CHANNEL.with(|slot| {
        // Safety: the slot is thread-local and only mutated through these helpers.
        unsafe {
            *slot.get() = None;
        }
    });
}

/// Returns the current thread's channel as a typed [`Channel`]; errors when
/// none is installed.
pub fn current_channel() -> RS<Channel> {
    CURRENT_CHANNEL.with(|slot| {
        // Safety: shared reads are confined to the current thread-local slot.
        let channel = unsafe { &*slot.get() };
        channel.as_ref().cloned().map(Channel::new).ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                "current channel is not set on this thread"
            )
        })
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::channel::endpoint::{ChannelEndpoint, KindHandler};
    use async_trait::async_trait;
    use mudu::channel::{Envelope, MessageId, MessageKind, OutgoingMessage, RecvFilter};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct StubEndpoint {
        local: EndpointId,
        closed: AtomicBool,
    }

    impl StubEndpoint {
        fn new(local: EndpointId) -> Arc<Self> {
            Arc::new(Self {
                local,
                closed: AtomicBool::new(false),
            })
        }
    }

    #[async_trait]
    impl ChannelEndpoint for StubEndpoint {
        fn local_endpoint(&self) -> EndpointId {
            self.local
        }

        async fn send(&self, _dst: EndpointId, _message: OutgoingMessage) -> RS<MessageId> {
            Ok(0)
        }

        async fn recv(&self, _filter: RecvFilter, _deadline_ms: Option<u64>) -> RS<Envelope> {
            Err(mudu_error!(ErrorCode::ChannelClosed, "stub endpoint"))
        }

        fn register_kind_handler(&self, _kind: MessageKind, _handler: KindHandler) -> RS<()> {
            Ok(())
        }

        fn unregister_kind_handler(&self, _kind: MessageKind) -> RS<()> {
            Ok(())
        }

        fn close(&self) {
            self.closed.store(true, Ordering::Relaxed);
        }
    }

    const SERVER: ServerInstanceId = 0x5E17_0000_0000_0000_0000_0000_0000_0001;

    #[test]
    fn spawn_endpoint_registers_and_guard_drop_unregisters_and_closes() {
        let endpoint = StubEndpoint::new(101);
        let closed = endpoint.clone();
        let channel: ChannelEndpointRef = endpoint;
        let (_typed, guard) = spawn_endpoint(SERVER, 101, &channel).unwrap();
        assert_eq!(guard.endpoint_id(), 101);
        assert!(channel_for(SERVER, 101).is_ok());
        drop(guard);
        assert!(closed.closed.load(Ordering::Relaxed));
        let err = channel_for(SERVER, 101).err().unwrap();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    }

    #[test]
    fn channel_for_unknown_endpoint_errors() {
        let err = channel_for(SERVER, 0xFFFF_EEEE).err().unwrap();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        // Unregistering an unknown endpoint is a no-op.
        unregister_channel(SERVER, 0xFFFF_EEEE).unwrap();
    }

    #[test]
    fn current_channel_tracks_the_thread_local_slot() {
        let err = current_channel().err().unwrap();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        let channel: ChannelEndpointRef = StubEndpoint::new(7);
        set_current_channel(channel);
        let typed = current_channel().unwrap();
        assert_eq!(typed.local_endpoint(), 7);
        unset_current_channel();
        assert!(current_channel().is_err());
    }

    #[test]
    fn request_ids_are_unique() {
        assert_ne!(gen_request_id(), gen_request_id());
    }

    #[test]
    fn spawn_endpoint_for_current_thread_uses_the_thread_oid() {
        // The main thread was not spawned through mudu_sys: no thread OID.
        let channel: ChannelEndpointRef = StubEndpoint::new(0);
        let err = spawn_endpoint_for_current_thread(SERVER, &channel)
            .err()
            .unwrap();
        assert_eq!(err.ec(), ErrorCode::InvalidState);

        let handle = mudu_sys::task::sync::spawn_thread_named("endpoint-thread", move || {
            let oid = mudu_sys::task::sync::try_this_thread_task_id().unwrap();
            let endpoint = StubEndpoint::new(oid);
            let closed = endpoint.clone();
            let channel: ChannelEndpointRef = endpoint;
            let (typed, guard) = spawn_endpoint_for_current_thread(SERVER, &channel).unwrap();
            assert_eq!(typed.local_endpoint(), oid);
            assert_eq!(guard.endpoint_id(), oid);
            assert!(channel_for(SERVER, oid).is_ok());
            drop(guard);
            assert!(closed.closed.load(Ordering::Relaxed));
            assert!(channel_for(SERVER, oid).is_err());
        })
        .unwrap();
        handle.join().unwrap();
    }
}
