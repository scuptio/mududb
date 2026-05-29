#[cfg(test)]
mod test {
    #[cfg(feature = "debug_trace")]
    use crate::debug::debug_serve_with_listener;
    #[cfg(not(feature = "debug_trace"))]
    use crate::debug::debug_serve;
    use crate::log::log_setup;
    use crate::notifier::notify_wait;
    #[cfg(feature = "debug_trace")]
    use crate::task_async::build_current_thread_runtime;
    use crate::task_sync::spawn_thread_named;
    #[cfg(feature = "debug_trace")]
    use std::io::{Read, Write};
    use std::net::SocketAddr;
    use std::time::Duration;

    #[test]
    fn test_server() {
        log_setup("info");
        let listener = match std::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))) {
            Ok(listener) => listener,
            Err(err) => {
                eprintln!("skip test_server: cannot bind local port: {err}");
                return;
            }
        };
        let addr: SocketAddr = listener.local_addr().unwrap();

        let (notifier, waiter) = notify_wait();
        let server_stop = waiter.into();
        #[cfg(feature = "debug_trace")]
        let (ready_notifier, ready_waiter) = notify_wait();
        #[cfg(feature = "debug_trace")]
        let server = spawn_thread_named("test_server", move || {
            debug_serve_with_listener(server_stop, listener, ready_notifier);
        })
        .unwrap();
        #[cfg(not(feature = "debug_trace"))]
        let server = {
            drop(listener);
            spawn_thread_named("test_server", move || {
                debug_serve(server_stop, addr.port());
            })
            .unwrap()
        };

        #[cfg(feature = "debug_trace")]
        {
            let runtime = build_current_thread_runtime().unwrap();
            runtime.block_on(async {
                ready_waiter.wait().await;
            });
        }

        #[cfg(feature = "debug_trace")]
        {
            let response = (|| -> std::io::Result<String> {
                let mut stream = std::net::TcpStream::connect(addr)?;
                stream.write_all(
                    b"GET /task HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
                )?;
                let mut buf = String::new();
                stream.read_to_string(&mut buf)?;
                Ok(buf)
            })()
            .expect("debug server did not accept requests");
            assert!(response.starts_with("HTTP/1.1 200"));
        }

        notifier.notify_all();
        for _ in 0..20 {
            if server.is_finished() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            server.is_finished(),
            "debug_serve thread did not stop after notify"
        );
        server.join().unwrap();
    }
}
