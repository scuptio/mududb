//! Per-worker registry of queue activations (P1): named procedures invoked
//! post-commit while a queue is non-empty.
//!
//! An activation is registered through the management HTTP API and is
//! **volatile**: it lives only in this worker's memory and is lost on restart
//! (clients re-register after a reboot; the RFC's catalog-registered
//! `ALTER QUEUE ... SET ACTIVATION` form is out of P1 scope). Exactly one
//! worker hosts a queue's (unpartitioned) partition, and the registration is
//! routed there — the activation fires on this worker's commits.
//!
//! The semantics are at-least-once: the commit apply path calls
//! [`ActivationRegistry::notify`] for every queue the commit wrote, and the
//! activation's driver task invokes the procedure once per wake while the
//! queue is non-empty. The procedure receives the queue name as its single
//! `String` parameter; it is expected to drain through `RECEIVE`/`ACK`
//! itself. A successful invocation followed by a still-non-empty queue
//! re-arms the driver after a short delay (the procedure may process a
//! bounded batch per call). A failed invocation retries with exponential
//! backoff; after `max_retry` consecutive failures the activation disables
//! itself (an alarm is logged) and parks until it is re-registered or
//! unregistered — a failing procedure can never storm the worker.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{error, warn};

use mudu_sys::sync::async_::{stop_channel, ANotify, StopRx, StopTx};
use mudu_sys::task::async_::{build_current_thread_runtime, LocalTaskSet};
use mudu_sys::task::sync::spawn_thread_named;

use crate::queue_types::QueueRegistry;
use mudu::common::id::OID;
use mudu::common::result::RS;
#[cfg(test)]
use mudu::error::ErrorCode;
#[cfg(test)]
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Default consecutive-failure budget of an activation before it disables
/// itself (`max_retry` when the registration does not carry one).
pub const QUEUE_ACTIVATION_MAX_RETRY_DEFAULT: u64 = 5;
/// Default batch size of an activation registration (`batch` when the
/// registration does not carry one). P1 stores and reports it only; the
/// registered procedure decides how much it drains per invocation.
pub const QUEUE_ACTIVATION_BATCH_DEFAULT: u64 = 32;
/// Delay before re-invoking after a successful invocation left the queue
/// non-empty (the procedure may drain a bounded batch per call).
const ACTIVATION_REARM_DELAY: Duration = Duration::from_millis(50);
/// Base of the failure backoff: 100ms, doubling per consecutive failure,
/// capped at 30s.
const ACTIVATION_BACKOFF_BASE: Duration = Duration::from_millis(100);
const ACTIVATION_BACKOFF_CAP: Duration = Duration::from_secs(30);

/// The injected invocation capability of one queue activation: the server
/// runtime (mudu_runtime) implements it over the WASM procedure host.
///
/// `invoke` runs the registered procedure with the queue name as its single
/// string argument. Guest-side failures must surface as `Err` (the procedure
/// host reports them inside the serialized result rather than as the invoke
/// error; the implementation unwraps that).
#[async_trait]
pub trait QueueActivationInvoker: Send + Sync {
    /// Invokes the activation's procedure for the queue `queue_name`.
    async fn invoke(&self, queue_name: &str) -> RS<()>;
}

/// One queue activation registration (`POST /mudu/server/queue_activation`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueueActivationRegistration {
    /// Table OID of the queue.
    pub queue_oid: OID,
    /// Name of the queue (the procedure's single argument).
    pub queue_name: String,
    /// Procedure to invoke, in `app/module/procedure` form.
    pub procedure: String,
    /// Consecutive failures tolerated before the activation disables itself.
    pub max_retry: u64,
    /// Stored/observed batch size hint (not enforced by the driver).
    pub batch: u64,
}

/// Point-in-time status of one registered queue activation, for the
/// observability endpoints.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueueActivationStatus {
    /// Table OID of the queue.
    pub queue_oid: OID,
    /// Name of the queue.
    pub queue_name: String,
    /// Registered procedure, in `app/module/procedure` form.
    pub procedure: String,
    /// Consecutive failures tolerated before the activation disables itself.
    pub max_retry: u64,
    /// Stored batch size hint.
    pub batch: u64,
    /// Whether the activation still fires (false after a failure storm
    /// exceeded `max_retry`).
    pub enabled: bool,
    /// Consecutive invocation failures since the last success.
    pub consecutive_failures: u64,
}

/// Mutable driver state of one activation, shared between the registry and
/// the driver task.
struct ActivationShared {
    spec: QueueActivationRegistration,
    state: SMutex<ActivationState>,
    notify: ANotify,
    stop: StopTx,
}

struct ActivationState {
    enabled: bool,
    consecutive_failures: u64,
}

/// Per-worker registry of queue activations, keyed by the queue's table OID.
/// Registrations are volatile (in-memory only); the commit apply path calls
/// [`ActivationRegistry::notify`] for every written queue.
pub struct ActivationRegistry {
    inner: SMutex<HashMap<OID, Arc<ActivationShared>>>,
    /// Live registration count; the commit hook consults it to skip the
    /// write-set scan entirely while no activation is registered.
    registration_count: AtomicUsize,
}

impl ActivationRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            inner: SMutex::new(HashMap::new()),
            registration_count: AtomicUsize::new(0),
        }
    }

    fn lock(&self) -> RS<mudu_sys::sync::SMutexGuard<'_, HashMap<OID, Arc<ActivationShared>>>> {
        self.inner.lock()
    }

    /// Whether any activation is registered; the commit hook uses this to
    /// skip the write-set scan while no activation exists.
    pub fn has_any(&self) -> bool {
        self.registration_count.load(Ordering::Relaxed) > 0
    }

    /// Registers (or replaces) the activation of one queue and spawns its
    /// driver task. The driver fires immediately when the queue already holds
    /// messages. `queue_registry` supplies the driver's depth reads.
    pub fn register(
        &self,
        registration: QueueActivationRegistration,
        invoker: Arc<dyn QueueActivationInvoker>,
        queue_registry: Arc<QueueRegistry>,
    ) -> RS<()> {
        let (stop_tx, stop_rx) = stop_channel();
        let shared = Arc::new(ActivationShared {
            spec: registration,
            state: SMutex::new(ActivationState {
                enabled: true,
                consecutive_failures: 0,
            }),
            notify: ANotify::new(),
            stop: stop_tx,
        });
        // Spawn before touching the map: a failure must not leave a
        // driverless entry behind. The driver owns a dedicated thread with
        // its own LocalSet-wrapped current-thread runtime: callers may run
        // on runtimes without one (the HTTP management endpoint's actix
        // workers), while procedure invocation spawns thread-confined
        // lifecycle tasks that require an active LocalSet.
        let task_name = format!("queue-activation-{}", shared.spec.queue_oid);
        let runtime = build_current_thread_runtime()?;
        let driver_shared = shared.clone();
        let driver_registry = queue_registry.clone();
        let _handle = spawn_thread_named(task_name, move || {
            LocalTaskSet::new().block_on(
                &runtime,
                drive_activation(driver_shared, invoker, driver_registry, stop_rx),
            )
        })?;
        let previous = {
            let mut guard = self.lock()?;
            let previous = guard.insert(shared.spec.queue_oid, shared.clone());
            if previous.is_none() {
                self.registration_count.fetch_add(1, Ordering::Relaxed);
            }
            previous
        };
        // An upsert stops the replaced driver (the new one owns the queue's
        // notifications from now on); the notification wakes the replaced
        // driver so it observes the stop promptly.
        if let Some(previous) = previous {
            previous.stop.stop();
            previous.notify.notify_waiters();
        }
        // Fire at once when the queue already holds messages: activations
        // registered onto a backlog must not wait for the next commit.
        let depth = match queue_registry.get((shared.spec.queue_oid, 0))? {
            Some(runtime) => runtime.depth(),
            None => 0,
        };
        if depth > 0 {
            shared.notify.notify_waiters();
        }
        Ok(())
    }

    /// Removes the activation of `queue_oid` and stops its driver task;
    /// `false` when the queue had no activation. The stop signal is paired
    /// with a notification so a parked driver wakes and exits promptly.
    pub fn unregister(&self, queue_oid: OID) -> RS<bool> {
        let removed = self.lock()?.remove(&queue_oid);
        match removed {
            Some(shared) => {
                shared.stop.stop();
                shared.notify.notify_waiters();
                self.registration_count.fetch_sub(1, Ordering::Relaxed);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Wakes the activation driver of `queue_oid`, if any. Cheap when nothing
    /// is registered: one atomic load short-circuits before the map is
    /// touched (the commit hook's fast path).
    pub fn notify(&self, queue_oid: OID) -> RS<()> {
        if !self.has_any() {
            return Ok(());
        }
        let shared = self.lock()?.get(&queue_oid).cloned();
        // ANotify wakeups are sticky: the driver observes the signal even when
        // it has not parked yet.
        if let Some(shared) = shared {
            shared.notify.notify_waiters();
        }
        Ok(())
    }

    /// Point-in-time status of every registered activation, sorted by queue
    /// OID, for the observability endpoints.
    pub fn list(&self) -> RS<Vec<QueueActivationStatus>> {
        let mut out = Vec::new();
        for (queue_oid, shared) in self.lock()?.iter() {
            let state = shared.state.lock()?;
            out.push(QueueActivationStatus {
                queue_oid: *queue_oid,
                queue_name: shared.spec.queue_name.clone(),
                procedure: shared.spec.procedure.clone(),
                max_retry: shared.spec.max_retry,
                batch: shared.spec.batch,
                enabled: state.enabled,
                consecutive_failures: state.consecutive_failures,
            });
        }
        out.sort_by_key(|status| status.queue_oid);
        Ok(out)
    }

    /// Stops every driver task and clears the registry (worker teardown).
    pub fn shutdown_all(&self) -> RS<()> {
        let entries: Vec<Arc<ActivationShared>> =
            self.lock()?.drain().map(|(_, shared)| shared).collect();
        for shared in entries {
            shared.stop.stop();
            shared.notify.notify_waiters();
        }
        self.registration_count.store(0, Ordering::Relaxed);
        Ok(())
    }
}

impl Default for ActivationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// The exponential backoff after `consecutive_failures` consecutive
/// invocation failures: 100ms, 200ms, 400ms, ... capped at 30s.
fn retry_backoff(consecutive_failures: u64) -> Duration {
    let exponent = consecutive_failures.saturating_sub(1).min(16) as u32;
    let millis = (ACTIVATION_BACKOFF_BASE.as_millis() as u64)
        .saturating_mul(2u64.pow(exponent))
        .min(ACTIVATION_BACKOFF_CAP.as_millis() as u64);
    Duration::from_millis(millis)
}

/// The driver loop of one activation: park on the wake latch (or a re-arm /
/// backoff timer), invoke the procedure once per wake while the queue is
/// non-empty, back off on failure, and disable after `max_retry` consecutive
/// failures. The loop only exits on the stop signal (unregister, queue drop,
/// worker teardown); stop paths pair the stop flag with a notification so a
/// parked driver wakes and observes it.
async fn drive_activation(
    shared: Arc<ActivationShared>,
    invoker: Arc<dyn QueueActivationInvoker>,
    queue_registry: Arc<QueueRegistry>,
    stop_rx: StopRx,
) {
    // `Some(delay)` parks the driver on a timer instead of the wake latch:
    // the re-arm delay after a successful invoke that left the queue
    // non-empty, or the failure backoff.
    let mut pending_delay: Option<Duration> = None;
    loop {
        match pending_delay.take() {
            // A timer wait that still observes notifications: a notify ends
            // the wait early (the latch is re-checked after the wake anyway).
            Some(delay) => {
                let _ = mudu_sys::task::async_::timeout(delay, shared.notify.notified()).await;
            }
            None => {
                shared.notify.notified().await;
            }
        }
        // Clear the sticky signal after the wake: a notification landing
        // mid-invoke re-arms the latch for the next round, and the depth
        // re-check below covers any notification consumed by this clear.
        shared.notify.clear_signal();
        if stop_rx.is_stopped() {
            return;
        }
        let Some(next_delay) = activation_round(&shared, invoker.as_ref(), &queue_registry).await
        else {
            return;
        };
        pending_delay = next_delay;
    }
}

/// One activation round after a wake: skip while disabled or while the queue
/// is empty, invoke once otherwise, and compute the next wait (re-arm delay,
/// backoff, or none). `None` ends the driver (a poisoned state lock is
/// unrecoverable).
async fn activation_round(
    shared: &Arc<ActivationShared>,
    invoker: &dyn QueueActivationInvoker,
    queue_registry: &Arc<QueueRegistry>,
) -> Option<Option<Duration>> {
    let queue_oid = shared.spec.queue_oid;
    let enabled = match shared.state.lock() {
        Ok(state) => state.enabled,
        Err(err) => {
            warn!(queue_oid, "queue activation state lock poisoned: {err}");
            return None;
        }
    };
    if !enabled {
        return Some(None);
    }
    // Skip the invoke while the queue is empty: the wake may have come from
    // an ACK commit rather than an enqueue.
    let depth = match queue_registry.get((queue_oid, 0)) {
        Ok(Some(runtime)) => runtime.depth(),
        Ok(None) => 0,
        Err(err) => {
            warn!(queue_oid, "queue activation depth read failed: {err}");
            0
        }
    };
    if depth == 0 {
        return Some(None);
    }
    let queue_name = shared.spec.queue_name.clone();
    match invoker.invoke(&queue_name).await {
        Ok(()) => {
            if let Ok(mut state) = shared.state.lock() {
                state.consecutive_failures = 0;
            }
            // The procedure may drain a bounded batch per call: re-arm while
            // the queue still holds messages instead of busy looping on a
            // synchronous invoke.
            let remaining = match queue_registry.get((queue_oid, 0)) {
                Ok(Some(runtime)) => runtime.depth(),
                _ => 0,
            };
            if remaining > 0 {
                return Some(Some(ACTIVATION_REARM_DELAY));
            }
            Some(None)
        }
        Err(err) => Some(activation_failure(shared, &queue_name, err)),
    }
}

/// Structs one invocation failure: bumps the consecutive-failure count, and
/// either arms the backoff timer or — past `max_retry` — disables the
/// activation with an alarm log. Returns the driver's next wait.
fn activation_failure(
    shared: &Arc<ActivationShared>,
    queue_name: &str,
    err: mudu::error::MuduError,
) -> Option<Duration> {
    let queue_oid = shared.spec.queue_oid;
    let (failures, max_retry) = match shared.state.lock() {
        Ok(mut state) => {
            state.consecutive_failures += 1;
            let failures = state.consecutive_failures;
            if failures >= shared.spec.max_retry {
                state.enabled = false;
            }
            (failures, shared.spec.max_retry)
        }
        Err(lock_err) => {
            warn!(
                queue_oid,
                "queue activation state lock poisoned: {lock_err}"
            );
            return None;
        }
    };
    if failures >= max_retry {
        error!(
            queue_oid,
            queue = %queue_name,
            procedure = %shared.spec.procedure,
            failures,
            "queue activation disabled after exhausting retries; re-register to re-enable: {err}"
        );
        None
    } else {
        warn!(
            queue_oid,
            queue = %queue_name,
            procedure = %shared.spec.procedure,
            failures,
            max_retry,
            "queue activation invocation failed; backing off: {err}"
        );
        Some(retry_backoff(failures))
    }
}

/// Registers (or replaces) the activation of one queue on this worker and
/// spawns its driver task. The caller (the management endpoint) routes to
/// the worker owning the queue's unpartitioned partition.
pub fn queue_activation_register(
    activation_registry: &ActivationRegistry,
    queue_registry: &Arc<QueueRegistry>,
    registration: QueueActivationRegistration,
    invoker: Arc<dyn QueueActivationInvoker>,
) -> RS<()> {
    activation_registry.register(registration, invoker, queue_registry.clone())
}

/// Removes the activation of `queue_oid` and stops its driver task;
/// `false` when the queue had no activation.
pub fn queue_activation_unregister(
    activation_registry: &ActivationRegistry,
    queue_oid: OID,
) -> RS<bool> {
    activation_registry.unregister(queue_oid)
}

/// Point-in-time status of every activation registered on this worker.
pub fn queue_activation_list(
    activation_registry: &ActivationRegistry,
) -> RS<Vec<QueueActivationStatus>> {
    activation_registry.list()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Fake invoker that records the queue names it was invoked with and
    /// "drains" the queue (advances the acked frontier past every allocated
    /// sequence) so a successful invoke leaves no re-arm behind.
    struct DrainingInvoker {
        calls: SMutex<Vec<String>>,
        queues: Arc<QueueRegistry>,
        queue_oid: OID,
    }

    #[async_trait]
    impl QueueActivationInvoker for DrainingInvoker {
        async fn invoke(&self, queue_name: &str) -> RS<()> {
            self.calls.lock()?.push(queue_name.to_string());
            let next = self
                .queues
                .get((self.queue_oid, 0))?
                .map(|runtime| runtime.next_seq())
                .unwrap_or(0);
            if next > 0 {
                self.queues
                    .advance_acked_frontier((self.queue_oid, 0), next - 1)?;
            }
            Ok(())
        }
    }

    /// Fake invoker that always fails.
    struct FailInvoker {
        calls: SMutex<Vec<String>>,
    }

    #[async_trait]
    impl QueueActivationInvoker for FailInvoker {
        async fn invoke(&self, queue_name: &str) -> RS<()> {
            self.calls.lock()?.push(queue_name.to_string());
            Err(mudu_error!(
                ErrorCode::Internal,
                "fake activation invoke failure"
            ))
        }
    }

    /// Fake invoker that succeeds but never drains: the driver re-arms
    /// itself while the queue stays non-empty.
    struct IdleInvoker {
        calls: SMutex<Vec<String>>,
    }

    #[async_trait]
    impl QueueActivationInvoker for IdleInvoker {
        async fn invoke(&self, queue_name: &str) -> RS<()> {
            self.calls.lock()?.push(queue_name.to_string());
            Ok(())
        }
    }

    fn registration(queue_oid: OID, max_retry: u64) -> QueueActivationRegistration {
        QueueActivationRegistration {
            queue_oid,
            queue_name: format!("q{queue_oid}"),
            procedure: "app/mod/proc".to_string(),
            max_retry,
            batch: QUEUE_ACTIVATION_BATCH_DEFAULT,
        }
    }

    /// Polls `pred` every 5ms until it holds or the 5s budget expires.
    async fn wait_until(pred: impl Fn() -> bool) {
        let deadline = mudu_sys::time::instant_now() + Duration::from_secs(5);
        while !pred() {
            assert!(
                mudu_sys::time::instant_now() < deadline,
                "condition did not become true within 5s"
            );
            mudu_sys::task::async_::sleep(Duration::from_millis(5))
                .await
                .unwrap();
        }
    }

    #[test]
    fn notify_drives_invoke_and_success_drains_without_rearm() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let registry = ActivationRegistry::new();
            let queues = Arc::new(QueueRegistry::new());
            // One message deep at registration: the driver fires immediately.
            queues.alloc((7, 0), 1, 100).unwrap();
            let invoker = Arc::new(DrainingInvoker {
                calls: SMutex::new(Vec::new()),
                queues: queues.clone(),
                queue_oid: 7,
            });
            registry
                .register(registration(7, 5), invoker.clone(), queues.clone())
                .unwrap();
            assert!(registry.has_any());
            wait_until(|| invoker.calls.lock().unwrap().len() == 1).await;
            assert_eq!(
                invoker.calls.lock().unwrap().as_slice(),
                &["q7".to_string()]
            );

            // The invoke drained the queue: no re-arm, no further calls.
            mudu_sys::task::async_::sleep(Duration::from_millis(150))
                .await
                .unwrap();
            assert_eq!(invoker.calls.lock().unwrap().len(), 1);

            // A commit-style notify on an empty queue skips the invoke;
            // a notify with new depth invokes again.
            registry.notify(7).unwrap();
            mudu_sys::task::async_::sleep(Duration::from_millis(50))
                .await
                .unwrap();
            assert_eq!(invoker.calls.lock().unwrap().len(), 1);
            queues.alloc((7, 0), 1, 100).unwrap();
            registry.notify(7).unwrap();
            wait_until(|| invoker.calls.lock().unwrap().len() == 2).await;

            registry.unregister(7).unwrap();
            assert!(!registry.has_any());
        })
        .unwrap()
    }

    #[test]
    fn register_on_empty_queue_waits_for_commit_notify() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let registry = ActivationRegistry::new();
            let queues = Arc::new(QueueRegistry::new());
            queues.register((7, 0)).unwrap();
            let invoker = Arc::new(DrainingInvoker {
                calls: SMutex::new(Vec::new()),
                queues: queues.clone(),
                queue_oid: 7,
            });
            registry
                .register(registration(7, 5), invoker.clone(), queues.clone())
                .unwrap();
            // Depth 0 at registration: no initial fire.
            mudu_sys::task::async_::sleep(Duration::from_millis(50))
                .await
                .unwrap();
            assert!(invoker.calls.lock().unwrap().is_empty());

            queues.alloc((7, 0), 1, 100).unwrap();
            registry.notify(7).unwrap();
            wait_until(|| invoker.calls.lock().unwrap().len() == 1).await;
            registry.unregister(7).unwrap();
        })
        .unwrap()
    }

    #[test]
    fn failures_backoff_and_disable_at_max_retry() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let registry = ActivationRegistry::new();
            let queues = Arc::new(QueueRegistry::new());
            queues.alloc((7, 0), 1, 100).unwrap();
            let invoker = Arc::new(FailInvoker {
                calls: SMutex::new(Vec::new()),
            });
            registry
                .register(registration(7, 3), invoker.clone(), queues.clone())
                .unwrap();
            // Failures 1..3 with 100ms/200ms backoffs in between, then the
            // activation disables itself.
            wait_until(|| {
                registry
                    .list()
                    .unwrap()
                    .first()
                    .is_some_and(|status| !status.enabled && status.consecutive_failures == 3)
            })
            .await;
            let calls_at_disable = invoker.calls.lock().unwrap().len();
            assert_eq!(calls_at_disable, 3);

            // Disabled: a later commit notify does not invoke again.
            registry.notify(7).unwrap();
            mudu_sys::task::async_::sleep(Duration::from_millis(300))
                .await
                .unwrap();
            assert_eq!(invoker.calls.lock().unwrap().len(), calls_at_disable);
            registry.unregister(7).unwrap();
        })
        .unwrap()
    }

    #[test]
    fn unregister_stops_the_driver_task() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let registry = ActivationRegistry::new();
            let queues = Arc::new(QueueRegistry::new());
            queues.alloc((7, 0), 1, 100).unwrap();
            let invoker = Arc::new(IdleInvoker {
                calls: SMutex::new(Vec::new()),
            });
            registry
                .register(registration(7, 5), invoker.clone(), queues.clone())
                .unwrap();
            // The queue never drains: the driver re-arms and keeps invoking.
            wait_until(|| invoker.calls.lock().unwrap().len() >= 2).await;
            assert!(registry.unregister(7).unwrap());
            let calls_at_unregister = invoker.calls.lock().unwrap().len();
            mudu_sys::task::async_::sleep(Duration::from_millis(200))
                .await
                .unwrap();
            assert_eq!(invoker.calls.lock().unwrap().len(), calls_at_unregister);
            // A notify after the unregister is a no-op.
            registry.notify(7).unwrap();
            mudu_sys::task::async_::sleep(Duration::from_millis(100))
                .await
                .unwrap();
            assert_eq!(invoker.calls.lock().unwrap().len(), calls_at_unregister);
        })
        .unwrap()
    }

    #[test]
    fn notify_without_registration_is_a_noop() {
        let registry = ActivationRegistry::new();
        assert!(!registry.has_any());
        registry.notify(7).unwrap();
        assert!(registry.list().unwrap().is_empty());
        assert!(!registry.unregister(7).unwrap());
    }

    #[test]
    fn upsert_replaces_the_driver_and_resets_failures() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let registry = ActivationRegistry::new();
            let queues = Arc::new(QueueRegistry::new());
            queues.alloc((7, 0), 1, 100).unwrap();
            let failing = Arc::new(FailInvoker {
                calls: SMutex::new(Vec::new()),
            });
            registry
                .register(registration(7, 5), failing.clone(), queues.clone())
                .unwrap();
            wait_until(|| !failing.calls.lock().unwrap().is_empty()).await;

            // Re-register (the re-enable path after a failure storm): one
            // entry, a fresh driver, failures reset.
            let draining = Arc::new(DrainingInvoker {
                calls: SMutex::new(Vec::new()),
                queues: queues.clone(),
                queue_oid: 7,
            });
            registry
                .register(registration(7, 5), draining.clone(), queues.clone())
                .unwrap();
            assert_eq!(registry.list().unwrap().len(), 1);
            wait_until(|| !draining.calls.lock().unwrap().is_empty()).await;
            let status = registry.list().unwrap().remove(0);
            assert!(status.enabled);
            assert_eq!(status.consecutive_failures, 0);
            registry.unregister(7).unwrap();
            assert!(!registry.has_any());
        })
        .unwrap()
    }

    #[test]
    fn retry_backoff_doubles_and_caps() {
        assert_eq!(retry_backoff(1), Duration::from_millis(100));
        assert_eq!(retry_backoff(2), Duration::from_millis(200));
        assert_eq!(retry_backoff(3), Duration::from_millis(400));
        assert_eq!(retry_backoff(20), ACTIVATION_BACKOFF_CAP);
    }
}
