#[cfg(test)]
mod tests {
    use crate::support::*;
    use mudu_runtime::backend::mududb_cfg::ServerMode;
    use mudu_utils::notifier::NotifyWait;
    use std::io;
    use std::time::Duration;

    #[test]
    fn supports_server_mode_legacy_and_tokio_always_true() {
        assert!(supports_server_mode(ServerMode::Legacy));
        assert!(supports_server_mode(ServerMode::Tokio));
    }

    #[test]
    fn supports_server_mode_iouring_matches_sys() {
        assert_eq!(
            supports_server_mode(ServerMode::IOUring),
            mudu_sys::io_uring_available()
        );
    }

    #[test]
    fn is_permission_denied_true() {
        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let err = mudu::mudu_error!(mudu::error::ErrorCode::Network, "denied", io_err);
        assert!(is_permission_denied(&err));
    }

    #[test]
    fn is_permission_denied_false() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
        let err = mudu::mudu_error!(mudu::error::ErrorCode::Network, "missing", io_err);
        assert!(!is_permission_denied(&err));
    }

    #[test]
    fn is_permission_denied_false_with_no_source() {
        let err = mudu::mudu_error!(mudu::error::ErrorCode::Network, "no source");
        assert!(!is_permission_denied(&err));
    }

    #[test]
    fn temp_dir_contains_prefix_and_is_unique() {
        let a = temp_dir("test_prefix");
        let b = temp_dir("test_prefix");
        assert_ne!(a, b);
        let name_a = a.file_name().unwrap().to_str().unwrap();
        let name_b = b.file_name().unwrap().to_str().unwrap();
        assert!(name_a.starts_with("test_prefix-"));
        assert!(name_b.starts_with("test_prefix-"));
    }

    #[test]
    fn test_runtime_domain_lock_is_singleton() {
        let lock1 = test_runtime_domain_lock();
        let lock2 = test_runtime_domain_lock();
        assert!(std::ptr::eq(lock1, lock2));
    }

    #[test]
    fn test_listener_bind_local_yields_ephemeral_port() {
        let listener = TestListener::bind_local().unwrap().expect("bind failed");
        let port = listener.port().unwrap();
        assert!(port > 0);
    }

    #[test]
    fn into_inner_returns_listener() {
        let listener = TestListener::bind_local().unwrap().expect("bind failed");
        let port = listener.port().unwrap();
        let inner = listener.into_inner();
        assert_eq!(inner.local_addr().unwrap().port(), port);
    }

    #[test]
    fn wait_until_backend_ready_ok_when_notified() {
        let notify_wait = NotifyWait::new();
        let (notifier, waiter) = notify_wait.notify_wait();
        let handle = mudu_sys::task::sync::spawn_thread(move || {
            mudu_sys::task::sync::sleep_blocking(Duration::from_millis(10));
            notifier.notify_all();
        })
        .expect("spawn backend-ready test thread");
        let result = wait_until_backend_ready(waiter, "test-service", Duration::from_secs(5));
        handle.join().expect("join backend-ready test thread");
        assert!(result.is_ok());
    }

    #[test]
    fn wait_until_backend_ready_errors_on_timeout() {
        let notify_wait = NotifyWait::new();
        let (_notifier, waiter) = notify_wait.notify_wait();
        let result = wait_until_backend_ready(waiter, "test-service", Duration::from_millis(10));
        assert!(result.is_err());
    }

    // The debug server thread is intentionally detached; Miri requires all
    // threads to be joined, so skip this test under Miri.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn start_debug_server_starts_without_error() {
        let listener = TestListener::bind_local().unwrap().expect("bind failed");
        let port = listener.port().unwrap();
        // Release the port so the debug server can bind to it.
        drop(listener);
        assert!(start_debug_server(port).is_ok());
    }
}
