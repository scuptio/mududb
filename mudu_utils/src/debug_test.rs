//! Tests for the debug HTTP server helpers and their feature-gated stubs.
#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

#[cfg(not(feature = "debug_trace"))]
use crate::debug::{async_debug_serve, debug_serve};
use crate::debug::{async_debug_serve_until, register_debug_url};
use crate::notifier::NotifyWait;
use mudu::common::result::RS;
use std::net::SocketAddr;

fn echo_url(path: String) -> RS<String> {
    Ok(path)
}

#[test]
fn register_debug_url_does_not_panic() {
    register_debug_url("/test".to_string(), echo_url);
}

#[cfg(not(feature = "debug_trace"))]
#[test]
fn debug_serve_stub_does_not_panic() {
    let canceler = NotifyWait::new();
    debug_serve(canceler, 0);
}

#[cfg(not(feature = "debug_trace"))]
#[test]
fn async_debug_serve_stub_returns_ok() {
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let result = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        async_debug_serve(addr).await
    });
    assert!(result.is_ok());
}

#[cfg(feature = "debug_trace")]
#[test]
fn async_debug_serve_until_stops_immediately() {
    let stop = NotifyWait::new();
    // Stop the server before it accepts anything.
    stop.notify_all();

    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let result = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        async_debug_serve_until(addr, stop).await
    });
    assert!(result.is_ok());
}

// Uses real TCP sockets and a detached server thread; Miri's socket/thread
// support is not reliable enough for this test.
#[cfg(feature = "debug_trace")]
#[cfg_attr(miri, ignore)]
#[test]
fn debug_server_uses_registered_url_handler() {
    use crate::debug::debug_serve_with_listener;
    use crate::notifier::notify_wait;
    use crate::task_sync::spawn_thread_named;
    use mudu_sys::net::sync::StdTcpListener;
    use std::io::{Read, Write};
    use std::time::Duration;

    register_debug_url("/echo".to_string(), echo_url);

    let listener = match StdTcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!(
                "skip debug_server_uses_registered_url_handler: cannot bind local port: {err}"
            );
            return;
        }
    };
    let addr: SocketAddr = listener.local_addr().unwrap();

    let (stop_notifier, stop_waiter) = notify_wait();
    let server_stop = stop_waiter.into();
    let (ready_notifier, ready_waiter) = notify_wait();
    let server = spawn_thread_named("debug_echo_server", move || {
        debug_serve_with_listener(server_stop, listener, ready_notifier);
    })
    .unwrap();

    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        ready_waiter.wait().await;
    });

    let response = (|| -> RS<String> {
        let mut stream = mudu_sys::net::sync::connect_tcp(addr)?;
        stream
            .write_all(b"GET /echo HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .map_err(|e| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::from(&e),
                    "debug server write error",
                    e
                )
            })?;
        let mut buf = String::new();
        stream.read_to_string(&mut buf).map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::from(&e),
                "debug server read error",
                e
            )
        })?;
        Ok(buf)
    })()
    .expect("debug server did not accept requests");

    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("/echo"));

    stop_notifier.notify_all();
    for _ in 0..20 {
        if server.is_finished() {
            break;
        }
        mudu_sys::task::sync::sleep_blocking(Duration::from_millis(50));
    }
    assert!(
        server.is_finished(),
        "debug server thread did not stop after notify"
    );
    server.join().unwrap();
}
