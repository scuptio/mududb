#[cfg(test)]
mod tests {
    use crate::{
        connect_sync_client_with_retry, reserve_port, reserve_port_block, wait_until_port_ready,
    };
    use mudu_sys::net::sync::SStdTcpStream;
    use std::net::SocketAddr;

    #[test]
    fn reserve_port_returns_some_ephemeral_port() {
        let port = reserve_port().unwrap().expect("reserve_port returned None");
        assert!(port > 0);
    }

    #[test]
    fn reserve_port_block_zero_returns_none() {
        assert_eq!(reserve_port_block(0).unwrap(), None);
    }

    #[test]
    fn reserve_port_block_one_returns_some_port() {
        let base = reserve_port_block(1)
            .unwrap()
            .expect("reserve_port_block(1) returned None");
        assert!(base > 0);
    }

    #[test]
    fn reserve_port_block_returns_contiguous_free_ports() {
        let count = 5;
        let base = reserve_port_block(count)
            .unwrap()
            .expect("reserve_port_block returned None");
        assert!(base > 0);

        for offset in 0..count {
            let port = base + offset as u16;
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            let listener = mudu_sys::net::sync::StdTcpListener::bind(addr);
            assert!(
                listener.is_ok(),
                "port {} was not free after reserve_port_block",
                port
            );
            drop(listener.unwrap());
        }
    }

    #[test]
    fn wait_until_port_ready_succeeds_when_listening() {
        let listener =
            mudu_sys::net::sync::StdTcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
                .expect("bind listener");
        let port = listener.local_addr().unwrap().port();

        let handle = mudu_sys::task::sync::spawn_thread(move || {
            let _ = listener.accept();
        })
        .expect("spawn port-ready test thread");

        let result = wait_until_port_ready(port, "test-listener");
        // Wake up the accepting thread so the test terminates promptly.
        let _ = SStdTcpStream::connect(("127.0.0.1", port));
        handle.join().expect("join port-ready test thread");

        assert!(result.is_ok());
    }

    #[test]
    fn wait_until_port_ready_errors_when_port_unused() {
        // Reserve a port and release it so we know it is not currently listened on.
        let port = reserve_port().unwrap().expect("reserve failed");
        let result = wait_until_port_ready(port, "test-no-listener");
        assert!(result.is_err());
    }

    #[test]
    fn connect_sync_client_with_retry_errors_when_no_server() {
        let port = reserve_port().unwrap().expect("reserve failed");
        let result = connect_sync_client_with_retry(port);
        assert!(result.is_err());
    }
}
