#![warn(missing_docs)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

//! Integration-test helpers for the MuduDB workspace.

use mudu::common::result::RS;
use mudu_cli::client::client::SyncClient;
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
pub fn wait_until_port_ready(port: u16, service_name: &str) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + Duration::from_secs(10);
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

/// Connects a synchronous client to `port`, retrying until timeout.
pub fn connect_sync_client_with_retry(port: u16) -> RS<SyncClient> {
    let deadline = mudu_sys::time::instant_now() + Duration::from_secs(5);
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
