#[cfg(test)]
mod test {
    use crate::debug::debug_serve;
    use crate::log::log_setup;
    use crate::notifier::notify_wait;
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
        drop(listener);
        let port = addr.port();

        let (notifier, waiter) = notify_wait();
        let server_stop = waiter.into();
        let server = spawn_thread_named("test_server", move || {
            debug_serve(server_stop, port);
        })
        .unwrap();

        #[cfg(feature = "debug_trace")]
        {
            let mut response = None;
            for _ in 0..20 {
                std::thread::sleep(Duration::from_millis(50));
                let attempt = (|| -> std::io::Result<String> {
                    let mut stream = std::net::TcpStream::connect(addr)?;
                    stream.write_all(
                        b"GET /task HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
                    )?;
                    let mut buf = String::new();
                    stream.read_to_string(&mut buf)?;
                    Ok(buf)
                })();
                if let Ok(buf) = attempt {
                    response = Some(buf);
                    break;
                }
            }
            let response = response.expect("debug server did not accept requests");
            assert!(response.starts_with("HTTP/1.1 200"));
        }

        notifier.notify_all();
        for _ in 0..20 {
            if server.is_finished() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(server.is_finished(), "debug_serve thread did not stop after notify");
        server.join().unwrap();
    }
}
