use super::*;
use std::time::Duration;

async fn result(
    f: &Fixture,
    id: u128,
    stop: mudu_sys::tokio::sync::watch::Receiver<bool>,
) -> RS<()> {
    mudu_sys::timeout(Duration::from_secs(2), f.machine.wait_request(id, stop))
        .await
        .expect("result waiter was not notified")
}

#[test]
fn pending_and_late_waiters_observe_failed_cancelled_or_exited_apply() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        for mode in [1, 2, 3] {
            let f = Fixture::new().await;
            let (stop, _) = mudu_sys::tokio::sync::watch::channel(false);
            let trigger = async {
                let record = f.record(0).await;
                if mode == 3 {
                    drop(f.machine.failure_guard()); // Apply driver exited.
                } else {
                    f.applier.mode.store(mode, Ordering::SeqCst);
                    if mode == 1 {
                        assert!(f.machine.apply_once(10, record).await.is_err());
                    } else {
                        tokio::select! {
                            _ = f.machine.apply_once(10, record) => panic!("unexpected completion"),
                            _ = f.applier.entered.notified() => (),
                        }
                    }
                }
            };
            let (a, b, ()) = tokio::join!(
                result(&f, 10, stop.subscribe()),
                result(&f, 11, stop.subscribe()),
                trigger
            );
            assert_eq!(a.unwrap_err().ec(), ErrorCode::InvalidState);
            assert_eq!(b.unwrap_err().ec(), ErrorCode::InvalidState);
            assert!(result(&f, 10, stop.subscribe()).await.is_err());
            assert!(f.machine.ensure_available().is_err());
        }
    })
    .unwrap();
}

#[test]
fn shutdown_wakes_pending_waiters_but_keeps_applied_results() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        f.machine.apply_once(10, f.record(0).await).await.unwrap();
        let (stop, _) = mudu_sys::tokio::sync::watch::channel(false);
        let (pending, ()) = tokio::join!(result(&f, 11, stop.subscribe()), async {
            stop.send_replace(true);
        });
        assert_eq!(pending.unwrap_err().ec(), ErrorCode::InvalidState);
        assert!(result(&f, 11, stop.subscribe()).await.is_err());
        drop(f.machine.failure_guard());
        result(&f, 10, stop.subscribe()).await.unwrap();
    })
    .unwrap();
}

#[test]
fn success_notifies_all_waiters_and_closed_shutdown_channel_fails_pending() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        let (stop, _) = mudu_sys::tokio::sync::watch::channel(false);
        let record = f.record(0).await;
        let (a, b, apply) = tokio::join!(
            result(&f, 10, stop.subscribe()),
            result(&f, 10, stop.subscribe()),
            f.machine.apply_once(10, record)
        );
        a.unwrap();
        b.unwrap();
        apply.unwrap();
        let receiver = stop.subscribe();
        drop(stop);
        assert_eq!(
            result(&f, 11, receiver).await.unwrap_err().ec(),
            ErrorCode::InvalidState
        );
    })
    .unwrap();
}
