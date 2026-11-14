#![warn(missing_docs)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

//! Integration-test helpers for the MuduDB workspace.

use mudu::common::result::RS;
use mudu_client::client::client::SyncClient;
use mudu_sys::net::sync::StdTcpListener;
use std::net::SocketAddr;
use std::time::Duration;

/// Common helpers used by integration tests.
pub mod support;

#[cfg(test)]
mod lib_test;
#[cfg(test)]
mod support_test;

/// Binds a temporary TCP listener on `127.0.0.1:0` and returns its port.
pub fn reserve_port() -> RS<Option<u16>> {
    match StdTcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))) {
        Ok(listener) => Ok(Some(
            listener
                .local_addr()
                .map_err(|e| {
                    mudu::mudu_error!(mudu::error::ErrorCode::Network, "read local addr error", e)
                })?
                .port(),
        )),
        Err(e) => Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Network,
            "reserve local tcp port error",
            e
        )),
    }
}

/// Reserves a contiguous block of `count` TCP ports.
pub fn reserve_port_block(count: usize) -> RS<Option<u16>> {
    if count == 0 {
        return Ok(None);
    }
    for _ in 0..128 {
        let Some(base_port) = reserve_port()? else {
            return Ok(None);
        };
        let mut listeners = Vec::with_capacity(count);
        let mut ok = true;
        for offset in 0..count {
            let Some(port) = base_port.checked_add(offset as u16) else {
                ok = false;
                break;
            };
            match StdTcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))) {
                Ok(listener) => listeners.push(listener),
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            return Ok(Some(base_port));
        }
    }
    Ok(None)
}

/// Waits until a service starts accepting TCP connections on `port`.
///
/// The wait budget is [`support::backend_startup_timeout`]: booting a full
/// backend (including mpk auto-load, which compiles wasmtime components) is
/// ~15-25x slower under sanitizer instrumentation and also noticeably slower
/// under coverage instrumentation, so a fixed short timeout makes
/// instrumented runs fail with "server did not become ready".
pub fn wait_until_port_ready(port: u16, service_name: &str) -> RS<()> {
    wait_until_port_ready_within(port, service_name, support::backend_startup_timeout())
}

/// Waits until a service starts accepting TCP connections on `port`, giving
/// up after `timeout`.
pub fn wait_until_port_ready_within(port: u16, service_name: &str, timeout: Duration) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + timeout;
    while mudu_sys::time::instant_now() < deadline {
        if mudu_sys::net::sync::connect_tcp(SocketAddr::from(([127, 0, 0, 1], port))).is_ok() {
            return Ok(());
        }
        mudu_sys::task::sync::sleep_blocking(Duration::from_millis(25));
    }
    Err(mudu::mudu_error!(
        mudu::error::ErrorCode::Network,
        format!(
            "{} server did not become ready on port {}",
            service_name, port
        )
    ))
}

/// Retry budget for [`connect_sync_client_with_retry`]: 5 s on regular
/// builds, scaled up under sanitizer or coverage instrumentation, where the
/// worker's accept path is slow enough that connections keep being refused
/// well beyond 5 s even though the server is coming up.
fn client_connect_retry_budget() -> Duration {
    if support::sanitizer_runtime_detected() {
        Duration::from_secs(120)
    } else if support::coverage_runtime_detected() {
        Duration::from_secs(30)
    } else {
        Duration::from_secs(5)
    }
}

/// Connects a synchronous client to `port`, retrying until the budget from
/// `client_connect_retry_budget` expires.
pub fn connect_sync_client_with_retry(port: u16) -> RS<SyncClient> {
    connect_sync_client_with_retry_within(port, client_connect_retry_budget())
}

/// Connects a synchronous client to `port`, retrying until `timeout`
/// expires. Tests that expect the connect to fail should use this variant
/// with a short explicit timeout so they stay fast under instrumentation.
pub fn connect_sync_client_with_retry_within(port: u16, timeout: Duration) -> RS<SyncClient> {
    let deadline = mudu_sys::time::instant_now() + timeout;
    let mut last_err = None;
    while mudu_sys::time::instant_now() < deadline {
        match SyncClient::connect(SocketAddr::from(([127, 0, 0, 1], port))) {
            Ok(client) => return Ok(client),
            Err(err) => {
                last_err = Some(err);
                mudu_sys::task::sync::sleep_blocking(Duration::from_millis(50));
            }
        }
    }
    match last_err {
        Some(err) => Err(err),
        None => Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Network,
            format!("timed out connecting SyncClient to TCP port {}", port)
        )),
    }
}
