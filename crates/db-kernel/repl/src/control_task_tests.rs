//! Background executor lifecycle tests without network socket dependencies.
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[test]
#[allow(clippy::panic)]
fn replay_panic_is_reported_and_executor_survives() {
    let task = crate::background::spawn("panicking-replay", async {
        panic!("replay regression panic");
    })
    .unwrap();
    let error = task.join().unwrap_err().to_string();
    assert!(error.contains("panicking-replay"), "{error}");
    assert!(error.contains("replay regression panic"), "{error}");
    crate::background::spawn("subsequent-replay", async { Ok(()) })
        .unwrap()
        .join()
        .unwrap();
}
