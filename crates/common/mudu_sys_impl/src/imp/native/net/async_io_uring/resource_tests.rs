//! Exhaust descriptor capacity in a subprocess so concurrent tests retain their limits.
#![allow(clippy::unwrap_used)]

#[test]
fn descriptor_exhaustion_propagates_without_losing_existing_handles() {
    const MARKER: &str = "MUDU_DESCRIPTOR_EXHAUSTION_TEST";
    if std::env::var_os(MARKER).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "imp::native::net::async_io_uring::resource_tests::descriptor_exhaustion_propagates_without_losing_existing_handles", "--nocapture"])
            .env(MARKER, "1")
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        return;
    }
    use super::io_uring_listener::IoUringListener;
    use crate::imp::native::net::sync::{TcpListenerSync, TcpStreamSync};
    let address = "127.0.0.1:0".parse().unwrap();
    let listener = TcpListenerSync::bind(address).unwrap();
    listener.set_nonblocking(true).unwrap();
    let listener_address = listener.local_addr().unwrap();
    let peer = TcpStreamSync::connect(listener_address).unwrap();
    let ring_listener = IoUringListener::bind_listener(address).unwrap();
    let file = crate::imp::fs::sync::SFile::open(std::env::current_exe().unwrap()).unwrap();
    let mut previous = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // Only this dedicated child changes its soft limit. Restore before assertions,
    // including before LLVM writes its profile on process exit.
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut previous) },
        0
    );
    let limited = libc::rlimit {
        rlim_cur: 0,
        rlim_max: previous.rlim_max,
    };
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limited) }, 0);
    let failures = [
        TcpListenerSync::bind(address).is_err(),
        listener.try_clone().is_err(),
        listener.accept().is_err(),
        TcpStreamSync::connect(listener_address).is_err(),
        IoUringListener::bind_listener(address).is_err(),
        ring_listener.try_clone_listener().is_err(),
        file.try_clone().is_err(),
    ];
    assert_eq!(
        unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &previous) },
        0
    );
    assert_eq!(failures, [true; 7]);
    assert_eq!(listener.local_addr().unwrap(), listener_address);
    assert_eq!(peer.peer_addr().unwrap(), listener_address);
    assert!(listener.try_clone().is_ok());
    assert!(ring_listener.try_clone_listener().is_ok());
    assert!(file.try_clone().is_ok());
}

#[test]
fn listener_rejects_non_ip_address_family() {
    use super::io_uring_listener::IoUringListener;
    use crate::imp::native::net::sync::{TcpListenerSync, TcpStreamSync};
    let socket = socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None).unwrap();
    let listener = TcpListenerSync::from_inner(socket.into());
    assert!(listener.local_addr().is_err());
    let socket = socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None).unwrap();
    let listener = IoUringListener::new(socket.into());
    assert!(listener.local_addr().is_err());
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )
    .unwrap();
    let stream = TcpStreamSync::from_inner(socket.into());
    assert!(stream.peer_addr().is_err());
}
