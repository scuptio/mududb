#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;
use crate::callback_registry::{CallbackDomain, CallbackEventKey, CallbackTrigger};
use crate::server_cfg::ReplRole;
use crate::worker::WorkerRuntimeParams;
use crate::worker_registry::load_or_create_worker_registry;
use ::wal::worker_log::{WalSyncPolicy, WorkerLogBatching};
use mudu_sys::env_var::temp_dir;
#[cfg(not(feature = "ds"))]
use mudu_sys::imp::native::linux::io_uring::file::{close, flush, open, read, write};
#[cfg(not(feature = "ds"))]
use mudu_sys::io::socket::{
    accept, close as close_socket, connect, recv, send, shutdown, socket, IoSocket,
};
#[cfg(not(feature = "ds"))]
use mudu_sys::tokio::task::yield_now;
#[cfg(not(feature = "ds"))]
use mudu_sys::TaskJoinHandle;
use mudu_utils::oid::gen_oid;
#[cfg(not(feature = "ds"))]
use mudu_utils::task_async::spawn_task_detached;
#[cfg(not(feature = "ds"))]
use std::io::{Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

// io_uring worker-ring-loop internals: the deterministic backend's
// simulated ring executes SQEs synchronously at submit and does not
// model the CQE-driven wakeup/cooperative-budget semantics this loop
// exercises. Native backend only.
#[cfg(not(feature = "ds"))]
/// Regression test: a registry-spawned task must survive many
/// cooperative yields. tokio's own `yield_now` defers the waker to the
/// tokio scheduler (which never runs on worker threads) and parks the
/// task forever; `cooperative_yield_now` wakes the registry waker
/// directly, so the drain/poll cycle drives the task to completion.
#[test]
#[cfg_attr(miri, ignore)]
fn worker_ring_loop_spawned_task_survives_cooperative_yields() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let Some(mut loop_state) = test_worker_loop().await else {
            return;
        };
        set_current_worker_ring(loop_state.worker_local_ring.clone());
        let count = Arc::new(AtomicUsize::new(0));
        let count_task = count.clone();
        loop_state.spawn(None, async move {
            for _ in 0..50 {
                ::mudu_utils::yield_now::cooperative_yield_now().await;
                count_task.fetch_add(1, AtomicOrdering::SeqCst);
            }
            Ok(())
        });
        for _ in 0..10000 {
            loop_state
                .worker_local_ring
                .worker_task_registry()
                .drain_completions();
            let (completed, _more) = loop_state
                .worker_local_ring
                .worker_task_registry()
                .poll_ready_budget(usize::MAX);
            for task in completed {
                let result = task.into_result();
                assert!(result.is_ok(), "spawned task failed: {:?}", result.err());
            }
            if count.load(AtomicOrdering::SeqCst) == 50 {
                break;
            }
        }
        assert_eq!(count.load(AtomicOrdering::SeqCst), 50);
        unset_current_worker_ring();
        loop_state.ring.exit();
        mudu_sys::sync::sync_::blocking::close_fd(loop_state.mailbox_fd).unwrap();
    })
    .unwrap()
}

async fn test_worker_loop() -> Option<WorkerRingLoop> {
    let dir = temp_dir()
        .join(format!("worker_ring_loop_test_{}", gen_oid()))
        .to_string_lossy()
        .into_owned();
    let registry = load_or_create_worker_registry(&dir, 1).unwrap();
    let identity = registry.worker(0).cloned().unwrap();
    let worker = WorkerRuntime::new(WorkerRuntimeParams {
        identity,
        worker_count: 1,
        log_dir: dir.clone(),
        data_dir: dir.clone(),
        log_chunk_size: 4096,
        log_batching: WorkerLogBatching::default(),
        wal_sync_policy: WalSyncPolicy::Commit,
        procedure_runtime: None,
        registry,
        async_runtime: None,
        server_instance_id: 0,
        repl_role: ReplRole::Primary,
        repl_leader_hint: None,
        raft_share: None,
        archive_dir: None,
    })
    .await
    .unwrap();
    let mailbox_fd = mudu_sys::sync::sync_::blocking::eventfd().unwrap();
    match WorkerRingLoop::new(WorkerRingLoopArgs {
        worker,
        listener_fd: -1,
        mailbox_fd,
        mailbox: Arc::new(SegQueue::new()),
        mailboxes: vec![Arc::new(SegQueue::new())],
        mailbox_fds: vec![mailbox_fd],
        conn_id_alloc: Arc::new(AtomicU64::new(1)),
        recovery_coordinator: Arc::new(RecoveryCoordinator::new(1, None)),
        stop: Arc::new(AtomicBool::new(false)),
    }) {
        Ok(loop_state) => Some(loop_state),
        Err(_) => {
            let _ = mudu_sys::sync::sync_::blocking::close_fd(mailbox_fd);
            None
        }
    }
}

// Only used by the io_uring-loop tests that are gated out under `ds`.
#[cfg(not(feature = "ds"))]
async fn drive_ring_future<T>(
    loop_state: &mut WorkerRingLoop,
    handle: &TaskJoinHandle<Option<T>>,
) -> RS<()>
where
    T: Send + 'static,
{
    while !handle.is_finished() {
        loop_state.submit_user_ring_io_if_needed()?;
        let submitted = loop_state.ring.submit();
        if submitted < 0 {
            return Err(mudu_error!(
                ErrorCode::Network,
                format!("io_uring_submit error {}", submitted)
            ));
        }
        if loop_state.inflight.is_empty() {
            yield_now().await;
            continue;
        }
        let cqe = loop_state.ring.wait().map_err(|wait_rc| {
            mudu_error!(
                ErrorCode::Network,
                format!("io_uring_wait_cqe error {}", wait_rc)
            )
        })?;
        loop_state.process_cqe(cqe)?;
        yield_now().await;
    }
    Ok(())
}

// io_uring worker-ring-loop internals: the deterministic backend's
// simulated ring executes SQEs synchronously at submit and does not
// model the CQE-driven wakeup/cooperative-budget semantics this loop
// exercises. Native backend only.
#[cfg(not(feature = "ds"))]
// These tests drive the real Linux io_uring backend, which Miri cannot
// emulate (it does not support the io_uring syscalls/FFI). They are
// ignored under Miri and run only on native Linux builds.
#[test]
#[cfg_attr(miri, ignore)]
fn worker_ring_loop_executes_user_file_io_via_cqe() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let Some(mut loop_state) = test_worker_loop().await else {
            return;
        };
        set_current_worker_ring(loop_state.worker_local_ring.clone());

        let path = temp_dir().join(format!("iouring_file_io_{}", gen_oid()));
        let path_str = path.to_string_lossy().into_owned();

        let open_task = spawn_task_detached("test", {
            let path_str = path_str.clone();
            async move {
                open(
                    &path_str,
                    libc::O_CREAT | libc::O_RDWR | libc::O_TRUNC | libc::O_CLOEXEC,
                    0o644,
                )
                .await
            }
        })
        .unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &open_task)
            .await
            .unwrap();
        let file = open_task.await.unwrap().unwrap().unwrap();

        let fd = file.fd();
        let write_task = spawn_task_detached("test", async move {
            write(
                &mudu_sys::imp::native::linux::io_uring::file::IoFile::from_raw_fd(fd),
                b"hello iouring".to_vec(),
                0,
            )
            .await
        })
        .unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &write_task)
            .await
            .unwrap();
        assert_eq!(
            write_task.await.unwrap().unwrap().unwrap(),
            b"hello iouring".len()
        );

        let fd = file.fd();
        let flush_task = spawn_task_detached("test", async move {
            flush(&mudu_sys::imp::native::linux::io_uring::file::IoFile::from_raw_fd(fd)).await
        })
        .unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &flush_task)
            .await
            .unwrap();
        flush_task.await.unwrap().unwrap().unwrap();

        let fd = file.fd();
        let read_task = spawn_task_detached("test", async move {
            read(
                &mudu_sys::imp::native::linux::io_uring::file::IoFile::from_raw_fd(fd),
                13,
                0,
            )
            .await
        })
        .unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &read_task)
            .await
            .unwrap();
        assert_eq!(
            read_task.await.unwrap().unwrap().unwrap(),
            b"hello iouring".to_vec()
        );

        let close_task = spawn_task_detached("test", async move { close(file).await }).unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &close_task)
            .await
            .unwrap();
        close_task.await.unwrap().unwrap().unwrap();

        unset_current_worker_ring();
        loop_state.ring.exit();
        mudu_sys::sync::sync_::blocking::close_fd(loop_state.mailbox_fd).unwrap();
        let _ = mudu_sys::fs::sync::remove_file(&path);
    })
    .unwrap()
}

// io_uring worker-ring-loop internals: the deterministic backend's
// simulated ring executes SQEs synchronously at submit and does not
// model the CQE-driven wakeup/cooperative-budget semantics this loop
// exercises. Native backend only.
#[cfg(not(feature = "ds"))]
#[test]
#[cfg_attr(miri, ignore)]
fn worker_ring_loop_executes_user_socket_connect_io_via_cqe() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let Some(mut loop_state) = test_worker_loop().await else {
            return;
        };
        set_current_worker_ring(loop_state.worker_local_ring.clone());

        let listener = StdTcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = listener.local_addr().unwrap();
        let peer = mudu_sys::task::sync::spawn_thread(move || -> RS<()> {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, b"ping");
            stream.write_all(b"pong").unwrap();
            let mut eof = [0u8; 1];
            let read = stream.read(&mut eof).unwrap();
            assert_eq!(read, 0);
            Ok(())
        })
        .unwrap();

        let socket_task = spawn_task_detached("test", async {
            socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0).await
        })
        .unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &socket_task)
            .await
            .unwrap();
        let sock = socket_task.await.unwrap().unwrap().unwrap();

        let fd = sock.fd();
        let connect_task = spawn_task_detached("test", async move {
            connect(&IoSocket::from_raw_fd(fd), addr).await
        })
        .unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &connect_task)
            .await
            .unwrap();
        connect_task.await.unwrap().unwrap().unwrap();

        let fd = sock.fd();
        let send_task = spawn_task_detached("test", async move {
            send(&IoSocket::from_raw_fd(fd), b"ping".to_vec(), 0).await
        })
        .unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &send_task)
            .await
            .unwrap();
        assert_eq!(send_task.await.unwrap().unwrap().unwrap(), 4);

        let fd = sock.fd();
        let recv_task = spawn_task_detached("test", async move {
            recv(&IoSocket::from_raw_fd(fd), 4, 0).await
        })
        .unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &recv_task)
            .await
            .unwrap();
        assert_eq!(recv_task.await.unwrap().unwrap().unwrap(), b"pong".to_vec());

        let fd = sock.fd();
        let shutdown_task = spawn_task_detached("test", async move {
            shutdown(&IoSocket::from_raw_fd(fd), libc::SHUT_WR).await
        })
        .unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &shutdown_task)
            .await
            .unwrap();
        shutdown_task.await.unwrap().unwrap().unwrap();

        let close_task =
            spawn_task_detached("test", async move { close_socket(sock).await }).unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &close_task)
            .await
            .unwrap();
        close_task.await.unwrap().unwrap().unwrap();

        peer.join().unwrap().unwrap();

        unset_current_worker_ring();
        loop_state.ring.exit();
        mudu_sys::sync::sync_::blocking::close_fd(loop_state.mailbox_fd).unwrap();
    })
    .unwrap()
}

// io_uring worker-ring-loop internals: the deterministic backend's
// simulated ring executes SQEs synchronously at submit and does not
// model the CQE-driven wakeup/cooperative-budget semantics this loop
// exercises. Native backend only.
#[cfg(not(feature = "ds"))]
#[test]
#[cfg_attr(miri, ignore)]
fn worker_ring_loop_executes_user_socket_accept_io_via_cqe() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let Some(mut loop_state) = test_worker_loop().await else {
            return;
        };
        set_current_worker_ring(loop_state.worker_local_ring.clone());

        let listener = StdTcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = listener.local_addr().unwrap();
        let listener_fd = unsafe { libc::dup(listener.into_raw_fd()) };
        assert!(listener_fd >= 0);
        let listener_sock = IoSocket::from_raw_fd(listener_fd);

        let peer = mudu_sys::task::sync::spawn_thread(move || -> RS<()> {
            let mut stream = mudu_sys::net::sync::connect_tcp(addr).unwrap();
            stream.write_all(b"ping").unwrap();
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, b"pong");
            Ok(())
        })
        .unwrap();

        let accept_fd = listener_sock.fd();
        let accept_task = spawn_task_detached("test", async move {
            accept(&IoSocket::from_raw_fd(accept_fd)).await
        })
        .unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &accept_task)
            .await
            .unwrap();
        let (accepted, remote_addr) = accept_task.await.unwrap().unwrap().unwrap();
        assert_eq!(
            remote_addr.ip(),
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        );

        let accepted_fd = accepted.fd();
        let recv_task = spawn_task_detached("test", async move {
            recv(&IoSocket::from_raw_fd(accepted_fd), 4, 0).await
        })
        .unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &recv_task)
            .await
            .unwrap();
        assert_eq!(recv_task.await.unwrap().unwrap().unwrap(), b"ping".to_vec());

        let accepted_fd = accepted.fd();
        let send_task = spawn_task_detached("test", async move {
            send(&IoSocket::from_raw_fd(accepted_fd), b"pong".to_vec(), 0).await
        })
        .unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &send_task)
            .await
            .unwrap();
        assert_eq!(send_task.await.unwrap().unwrap().unwrap(), 4);

        let accepted_fd = accepted.fd();
        let shutdown_task = spawn_task_detached("test", async move {
            shutdown(&IoSocket::from_raw_fd(accepted_fd), libc::SHUT_WR).await
        })
        .unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &shutdown_task)
            .await
            .unwrap();
        shutdown_task.await.unwrap().unwrap().unwrap();

        let close_accepted_task =
            spawn_task_detached("test", async move { close_socket(accepted).await }).unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &close_accepted_task)
            .await
            .unwrap();
        close_accepted_task.await.unwrap().unwrap().unwrap();

        let close_listener_task =
            spawn_task_detached("test", async move { close_socket(listener_sock).await }).unwrap();
        yield_now().await;
        drive_ring_future(&mut loop_state, &close_listener_task)
            .await
            .unwrap();
        close_listener_task.await.unwrap().unwrap().unwrap();

        peer.join().unwrap().unwrap();

        unset_current_worker_ring();
        loop_state.ring.exit();
        mudu_sys::sync::sync_::blocking::close_fd(loop_state.mailbox_fd).unwrap();
    })
    .unwrap()
}

// io_uring worker-ring-loop internals: the deterministic backend's
// simulated ring executes SQEs synchronously at submit and does not
// model the CQE-driven wakeup/cooperative-budget semantics this loop
// exercises. Native backend only.
#[cfg(not(feature = "ds"))]
/// Reproduces the partition-RPC wait composition: a worker task parks in
/// `async_::timeout(..., wait.wait())` (as `bus.recv` does), the response
/// is delivered via `notify` from the loop thread (as
/// `WorkerMessageBusState::handle_incoming` does), and the next loop
/// iteration must complete the task with the value.
#[test]
#[cfg_attr(miri, ignore)]
fn worker_ring_loop_timeout_wait_completes_on_notify() {
    use mudu_sys::sync::async_::notify_wait::create_notify_wait;

    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let Some(mut loop_state) = test_worker_loop().await else {
            return;
        };
        set_current_worker_ring(loop_state.worker_local_ring.clone());

        let (notify, wait) = create_notify_wait::<u64>();
        let result = Arc::new(SMutex::new(None));
        let result_task = result.clone();
        loop_state.spawn(None, async move {
            let r = mudu_sys::task::async_::timeout(Duration::from_secs(10), wait.wait()).await;
            *result_task.lock()? = r.map(|inner| inner.ok().flatten());
            Ok(())
        });

        // First loop iteration: the task registers the ring timeout and
        // parks on the oneshot receiver.
        loop_state.poll_ready_worker_tasks().unwrap();
        assert!(result.lock().unwrap().is_none());

        // The response arrives: notify from the loop thread, then run the
        // next iteration (drain completions + poll ready tasks).
        notify.notify(42).unwrap();
        loop_state
            .worker_local_ring
            .worker_task_registry()
            .drain_completions();
        loop_state.poll_ready_worker_tasks().unwrap();
        assert_eq!(*result.lock().unwrap(), Some(Some(42)));

        unset_current_worker_ring();
        loop_state.ring.exit();
        mudu_sys::sync::sync_::blocking::close_fd(loop_state.mailbox_fd).unwrap();
    })
    .unwrap()
}

// io_uring worker-ring-loop internals: the deterministic backend's
// simulated ring executes SQEs synchronously at submit and does not
// model the CQE-driven wakeup/cooperative-budget semantics this loop
// exercises. Native backend only.
#[cfg(not(feature = "ds"))]
/// Regression test for the tokio coop-budget exhaustion deadlock: io_uring
/// worker threads poll tasks manually inside a `block_on` whose coop
/// budget (128 by default) is only replenished by the tokio scheduler,
/// which never runs there. Once the budget is exhausted, tokio primitives
/// (e.g. the mutex inside notify_wait) return Pending from `poll_proceed`
/// and defer the waker to the never-driven runtime — the task parks
/// forever even though its resource is ready. The server drives workers
/// under `tokio::task::unconstrained`; this test mirrors that and checks
/// that a notify-wait task still completes after hundreds of
/// budget-consuming operations. Driving is bounded, so if the
/// `unconstrained` wrapper is dropped the final assertion fails instead
/// of hanging.
#[test]
#[cfg_attr(miri, ignore)]
fn worker_ring_loop_tasks_survive_coop_budget_exhaustion() {
    use mudu_sys::sync::async_::mutex::AMutex;
    use mudu_sys::sync::async_::notify_wait::create_notify_wait;

    mudu_sys::task::async_::block_on_tokio_current_thread(mudu_sys::tokio::task::unconstrained(
        async move {
            let Some(mut loop_state) = test_worker_loop().await else {
                return;
            };
            set_current_worker_ring(loop_state.worker_local_ring.clone());

            // Burn far more than tokio's default coop budget with
            // budget-consuming mutex acquires.
            let drained = Arc::new(AtomicUsize::new(0));
            let drained_task = drained.clone();
            loop_state.spawn(None, async move {
                let mutex = AMutex::new(0u32);
                for _ in 0..300 {
                    let mut guard = mutex.lock().await;
                    *guard += 1;
                    drop(guard);
                }
                drained_task.fetch_add(1, AtomicOrdering::SeqCst);
                Ok(())
            });
            loop_state.poll_ready_worker_tasks().unwrap();
            assert_eq!(drained.load(AtomicOrdering::SeqCst), 1);

            // notify_wait parks on a tokio mutex before reaching the
            // receiver; with an exhausted coop budget this is where the
            // task would be deferred forever.
            let (notify, wait) = create_notify_wait::<u64>();
            let result = Arc::new(SMutex::new(None));
            let result_task = result.clone();
            loop_state.spawn(None, async move {
                let r = mudu_sys::task::async_::timeout(Duration::from_secs(10), wait.wait()).await;
                *result_task.lock()? = r.map(|inner| inner.ok().flatten());
                Ok(())
            });
            loop_state.poll_ready_worker_tasks().unwrap();
            assert!(result.lock().unwrap().is_none());

            notify.notify(7).unwrap();
            loop_state
                .worker_local_ring
                .worker_task_registry()
                .drain_completions();
            loop_state.poll_ready_worker_tasks().unwrap();
            assert_eq!(*result.lock().unwrap(), Some(Some(7)));

            unset_current_worker_ring();
            loop_state.ring.exit();
            mudu_sys::sync::sync_::blocking::close_fd(loop_state.mailbox_fd).unwrap();
        },
    ))
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn worker_ring_loop_runs_event_callback_as_system_task() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let Some(mut loop_state) = test_worker_loop().await else {
            return;
        };
        let hit = Arc::new(AtomicUsize::new(0));
        let hit_clone = hit.clone();
        let callback_id = loop_state
            .register_async_callback(
                CallbackTrigger::Event(CallbackEventKey { kind: 7, id: 99 }),
                Box::new(move || {
                    Box::pin(async move {
                        hit_clone.fetch_add(1, AtomicOrdering::SeqCst);
                        Ok(())
                    })
                }),
            )
            .unwrap();
        assert!(callback_id > 0);

        loop_state
            .fire_callback_event(CallbackEventKey { kind: 7, id: 99 })
            .unwrap();
        loop_state.poll_ready_worker_tasks().unwrap();
        assert_eq!(hit.load(AtomicOrdering::SeqCst), 1);

        loop_state.ring.exit();
        mudu_sys::sync::sync_::blocking::close_fd(loop_state.mailbox_fd).unwrap();
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn worker_ring_loop_runs_sequence_callback_when_frontier_advances_and_skips_cancelled() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let Some(mut loop_state) = test_worker_loop().await else {
            return;
        };
        let hit = Arc::new(AtomicUsize::new(0));

        let first_hit = hit.clone();
        loop_state
            .register_async_callback(
                CallbackTrigger::Sequence {
                    domain: CallbackDomain::Generic(3),
                    target: 4,
                },
                Box::new(move || {
                    Box::pin(async move {
                        first_hit.fetch_add(1, AtomicOrdering::SeqCst);
                        Ok(())
                    })
                }),
            )
            .unwrap();

        let cancelled_hit = hit.clone();
        let cancelled = loop_state
            .register_async_callback(
                CallbackTrigger::Sequence {
                    domain: CallbackDomain::Generic(3),
                    target: 5,
                },
                Box::new(move || {
                    Box::pin(async move {
                        cancelled_hit.fetch_add(100, AtomicOrdering::SeqCst);
                        Ok(())
                    })
                }),
            )
            .unwrap();
        assert!(loop_state.cancel_async_callback(cancelled));

        loop_state
            .advance_callback_sequence(CallbackDomain::Generic(3), 4)
            .unwrap();
        loop_state.poll_ready_worker_tasks().unwrap();
        assert_eq!(hit.load(AtomicOrdering::SeqCst), 1);

        loop_state
            .advance_callback_sequence(CallbackDomain::Generic(3), 5)
            .unwrap();
        loop_state.poll_ready_worker_tasks().unwrap();
        assert_eq!(hit.load(AtomicOrdering::SeqCst), 1);

        loop_state.ring.exit();
        mudu_sys::sync::sync_::blocking::close_fd(loop_state.mailbox_fd).unwrap();
    })
    .unwrap()
}
