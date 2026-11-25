//! WebSocket gateway: long-lived client connections that invoke MuduDB
//! procedures and receive server-pushed events.
//!
//! The gateway is a thin serving frontend over a backend that invokes
//! procedures ([`WsBackend::invoke_json`]) and long-polls queue receives
//! ([`WsBackend::receive_queue`]). Inbound frames are procedure intents;
//! outbound frames are invocation results or zone-scoped broadcasts. A
//! connection joins a zone via the `queue` query parameter, and the gateway
//! runs one receive loop per zone that fans `RECEIVE ... WAIT FOREVER`
//! results out to every session subscribed to that zone.

mod session;

pub use session::{SessionId, SessionRegistry};

use actix_web::{web, HttpRequest, HttpResponse};
use futures::StreamExt;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_sys::sync::SMutex;
use mudu_sys::tokio::sync::watch;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// The procedure/queue backend the gateway relays to.
#[async_trait::async_trait(?Send)]
pub trait WsBackend: Send + Sync {
    /// Invokes a procedure with a JSON object body, returning JSON.
    async fn invoke_json(&self, app: &str, module: &str, proc: &str, body: String) -> RS<Value>;

    /// Receives up to one payload row from `queue`, optionally blocking
    /// forever for a message, decoded as JSON objects.
    async fn receive_queue(
        &self,
        queue: &str,
        consumer: Option<&str>,
        wait_forever: bool,
    ) -> RS<Vec<Value>>;
}

/// A client → server request frame (JSON text).
#[derive(Debug, Deserialize)]
pub struct ClientRequest {
    /// The installed application (mpk) name.
    pub app: String,
    /// The procedure module name.
    pub module: String,
    /// The procedure name.
    pub proc: String,
    /// The procedure argument object (JSON).
    pub body: serde_json::Value,
}

/// A server → client result frame.
#[derive(Debug, Serialize)]
struct ResultFrame {
    op: &'static str,
    ok: bool,
    data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// A server → client pushed event frame.
#[derive(Debug, Serialize)]
struct EventFrame {
    op: &'static str,
    zone: String,
    data: serde_json::Value,
}

/// The WebSocket gateway: owns the procedure backend and the session registry.
pub struct WsGateway {
    backend: Arc<dyn WsBackend>,
    sessions: Arc<SessionRegistry>,
    /// Zones with a running receive loop (fan-out per zone), each mapped to
    /// its loop's stop signal. The loop exits — and the entry is removed —
    /// when the zone's last session disconnects or the queue goes away, so
    /// zones never accumulate idle loops.
    receive_loops: SMutex<HashMap<String, watch::Sender<bool>>>,
}

impl WsGateway {
    /// Creates a gateway bound to the given procedure backend.
    pub fn new(backend: Arc<dyn WsBackend>) -> Arc<Self> {
        Arc::new(Self {
            backend,
            sessions: Arc::new(SessionRegistry::new()),
            receive_loops: SMutex::new(HashMap::new()),
        })
    }

    /// Registers the `/ws` route into an actix service config.
    pub fn configure(self: &Arc<Self>, cfg: &mut web::ServiceConfig) {
        let gateway = self.clone();
        cfg.service(web::resource("/ws").route(web::get().to(move |req, body| {
            let gateway = gateway.clone();
            async move { ws_handler(req, body, gateway).await }
        })));
    }

    /// Pushes a JSON text frame to every session subscribed to `zone`.
    ///
    /// Returns how many sessions received it.
    pub fn broadcast(&self, zone: &str, data: serde_json::Value) -> usize {
        let frame = EventFrame {
            op: "event",
            zone: zone.to_string(),
            data,
        };
        match serde_json::to_string(&frame) {
            Ok(text) => self.sessions.broadcast(zone, text),
            Err(_) => 0,
        }
    }

    /// Returns the session registry, for monitoring/tests.
    pub fn sessions(&self) -> &Arc<SessionRegistry> {
        &self.sessions
    }

    /// Starts the zone's receive loop unless one is already running.
    fn ensure_receive_loop(self: &Arc<Self>, zone: &str) {
        let mut loops = self
            .receive_loops
            .lock()
            .expect("receive-loop registry poisoned");
        if loops.contains_key(zone) {
            return;
        }
        let (stop_tx, stop_rx) = watch::channel(false);
        loops.insert(zone.to_string(), stop_tx);
        spawn_receive_loop(self.clone(), zone.to_string(), stop_rx);
    }

    /// Stops the zone's receive loop once its last session has disconnected.
    /// The loop's own exit path removes the registry entry, so a zone that
    /// later gains a session again starts a fresh loop.
    fn maybe_stop_receive_loop(&self, zone: &str) {
        if zone.is_empty() {
            return;
        }
        let mut loops = self
            .receive_loops
            .lock()
            .expect("receive-loop registry poisoned");
        if matches!(self.sessions.count(zone), Ok(0)) {
            if let Some(stop) = loops.remove(zone) {
                let _ = stop.send_replace(true);
            }
        }
    }
}

/// The actix WebSocket handshake entry point.
async fn ws_handler(
    req: HttpRequest,
    body: web::Payload,
    gateway: Arc<WsGateway>,
) -> Result<HttpResponse, actix_web::Error> {
    let (response, session, stream) = actix_ws::handle(&req, body)?;
    // The zone a connection joins is the queue named by the `queue` query
    // parameter; without one the connection only invokes procedures.
    let zone = web::Query::<HashMap<String, String>>::from_query(req.query_string())
        .ok()
        .and_then(|query| query.get("queue").cloned())
        .unwrap_or_default();
    let (tx, rx) = mpsc::channel::<String>(64);
    let session_id = gateway.sessions.register(zone.clone(), tx);
    if !zone.is_empty() {
        gateway.ensure_receive_loop(&zone);
    }
    actix_web::rt::spawn(session_loop(session, stream, rx, gateway, session_id, zone));
    Ok(response)
}

/// The per-connection loop: multiplexes client frames and server pushes.
async fn session_loop(
    mut session: actix_ws::Session,
    mut stream: actix_ws::MessageStream,
    mut rx: mpsc::Receiver<String>,
    gateway: Arc<WsGateway>,
    session_id: SessionId,
    zone: String,
) {
    loop {
        tokio::select! {
            frame = stream.next() => {
                match frame {
                    Some(Ok(actix_ws::Message::Text(text))) => {
                        handle_client_text(&mut session, gateway.backend.as_ref(), text.as_ref()).await;
                    }
                    Some(Ok(actix_ws::Message::Ping(bytes))) => {
                        let _ = session.pong(&bytes).await;
                    }
                    Some(Ok(actix_ws::Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            push = rx.recv() => {
                match push {
                    Some(text) => { let _ = session.text(text).await; }
                    None => break,
                }
            }
        }
    }
    gateway.sessions.unregister(session_id);
    gateway.maybe_stop_receive_loop(&zone);
}

/// The per-zone receive loop: long-polls `RECEIVE ... WAIT FOREVER` and
/// broadcasts each received row to every session in the zone. The loop exits
/// when the zone's last session disconnected (stop signal) or the queue is
/// gone; a transient failure retries after a short backoff. On exit the loop
/// removes its registry entry so a later connection starts a fresh loop.
fn spawn_receive_loop(gateway: Arc<WsGateway>, zone: String, mut stop: watch::Receiver<bool>) {
    actix_web::rt::spawn(async move {
        let backend = gateway.backend.clone();
        let sessions = gateway.sessions.clone();
        loop {
            let result = mudu_sys::tokio::select! {
                biased;
                _ = stop.changed() => break,
                result = backend.receive_queue(&zone, None, true) => result,
            };
            match result {
                Ok(rows) => {
                    for row in rows {
                        let frame = EventFrame {
                            op: "event",
                            zone: zone.clone(),
                            data: row,
                        };
                        if let Ok(text) = serde_json::to_string(&frame) {
                            sessions.broadcast(&zone, text);
                        }
                    }
                }
                Err(err) => {
                    if err.ec() == ErrorCode::ObjectNotFound {
                        // The queue was dropped (or never existed): a permanent
                        // failure, so exit instead of retrying forever.
                        break;
                    }
                    // A transient failure (worker restart, network): yield
                    // before retrying so a tight loop does not spin.
                    let _ = mudu_sys::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
        if let Ok(mut loops) = gateway.receive_loops.lock() {
            loops.remove(&zone);
        }
    });
}

/// Parses a client request and invokes the corresponding procedure.
async fn handle_client_text(session: &mut actix_ws::Session, backend: &dyn WsBackend, text: &str) {
    let frame = match serde_json::from_str::<ClientRequest>(text) {
        Ok(request) => invoke_and_frame(backend, request).await,
        Err(err) => ResultFrame {
            op: "result",
            ok: false,
            data: serde_json::Value::Null,
            error: Some(format!("invalid request: {err}")),
        },
    };
    if let Ok(serialized) = serde_json::to_string(&frame) {
        let _ = session.text(serialized).await;
    }
}

/// Invokes the procedure and encodes the outcome as a result frame.
async fn invoke_and_frame(backend: &dyn WsBackend, request: ClientRequest) -> ResultFrame {
    let body = match serde_json::to_string(&request.body) {
        Ok(body) => body,
        Err(err) => {
            return ResultFrame {
                op: "result",
                ok: false,
                data: serde_json::Value::Null,
                error: Some(format!("encode body: {err}")),
            }
        }
    };
    match backend
        .invoke_json(&request.app, &request.module, &request.proc, body)
        .await
    {
        Ok(data) => ResultFrame {
            op: "result",
            ok: true,
            data,
            error: None,
        },
        Err(err) => ResultFrame {
            op: "result",
            ok: false,
            data: serde_json::Value::Null,
            error: Some(err.to_string()),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use mudu::mudu_error;

    /// A backend whose queue receive parks forever (an idle queue).
    struct PendingBackend;

    #[async_trait::async_trait(?Send)]
    impl WsBackend for PendingBackend {
        async fn invoke_json(
            &self,
            _app: &str,
            _module: &str,
            _proc: &str,
            _body: String,
        ) -> RS<Value> {
            Err(mudu_error!(ErrorCode::NotImplemented, "no procedures"))
        }

        async fn receive_queue(
            &self,
            _queue: &str,
            _consumer: Option<&str>,
            _wait_forever: bool,
        ) -> RS<Vec<Value>> {
            std::future::pending().await
        }
    }

    /// A backend whose queue reports ObjectNotFound (a dropped queue).
    struct GoneBackend;

    #[async_trait::async_trait(?Send)]
    impl WsBackend for GoneBackend {
        async fn invoke_json(
            &self,
            _app: &str,
            _module: &str,
            _proc: &str,
            _body: String,
        ) -> RS<Value> {
            Err(mudu_error!(ErrorCode::NotImplemented, "no procedures"))
        }

        async fn receive_queue(
            &self,
            _queue: &str,
            _consumer: Option<&str>,
            _wait_forever: bool,
        ) -> RS<Vec<Value>> {
            Err(mudu_error!(ErrorCode::ObjectNotFound, "no such queue"))
        }
    }

    fn loop_count(gateway: &WsGateway) -> usize {
        gateway.receive_loops.lock().unwrap().len()
    }

    /// Waits until every receive loop exited; returns false on timeout.
    async fn loops_drained(gateway: &WsGateway) -> bool {
        for _ in 0..200 {
            if loop_count(gateway) == 0 {
                return true;
            }
            let _ = mudu_sys::task::async_::sleep(std::time::Duration::from_millis(5)).await;
        }
        false
    }

    #[test]
    fn receive_loop_stops_and_deregisters_when_zone_empties() {
        actix_web::rt::System::new().block_on(async {
            let gateway = WsGateway::new(Arc::new(PendingBackend));
            let (tx, _rx) = mpsc::channel(1);
            let id = gateway.sessions.register("z".to_string(), tx);
            gateway.ensure_receive_loop("z");
            assert_eq!(loop_count(&gateway), 1);

            gateway.sessions.unregister(id);
            gateway.maybe_stop_receive_loop("z");
            assert!(loops_drained(&gateway).await);
        });
    }

    #[test]
    fn receive_loop_survives_while_a_session_remains() {
        actix_web::rt::System::new().block_on(async {
            let gateway = WsGateway::new(Arc::new(PendingBackend));
            let (tx_a, _rx_a) = mpsc::channel(1);
            let (tx_b, _rx_b) = mpsc::channel(1);
            let id_a = gateway.sessions.register("z".to_string(), tx_a);
            let _id_b = gateway.sessions.register("z".to_string(), tx_b);
            gateway.ensure_receive_loop("z");
            gateway.ensure_receive_loop("z");
            assert_eq!(loop_count(&gateway), 1);

            // One session leaves: the loop must keep running for the other.
            gateway.sessions.unregister(id_a);
            gateway.maybe_stop_receive_loop("z");
            let _ = mudu_sys::task::async_::sleep(std::time::Duration::from_millis(20)).await;
            assert_eq!(loop_count(&gateway), 1);
        });
    }

    #[test]
    fn receive_loop_exits_when_the_queue_is_gone() {
        actix_web::rt::System::new().block_on(async {
            let gateway = WsGateway::new(Arc::new(GoneBackend));
            gateway.ensure_receive_loop("z");
            assert!(loops_drained(&gateway).await);
        });
    }
}
