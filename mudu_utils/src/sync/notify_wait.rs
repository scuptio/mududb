pub use mudu_sys::sync::notify_wait::*;

#[cfg(test)]
mod tests {
    use super::create_notify_wait;

    #[test]
    fn notify_wait_delivers_value_once() {
        let runtime = mudu_sys::task_async::build_current_thread_runtime().unwrap();
        runtime.block_on(async {
            let (notify, wait) = create_notify_wait::<u32>();
            assert!(notify.notify(7).unwrap());
            assert_eq!(wait.wait().await.unwrap(), Some(7));
            assert_eq!(wait.wait().await.unwrap(), Some(7));
        });
    }

    #[test]
    fn notify_returns_false_after_receiver_is_dropped() {
        let (notify, wait) = create_notify_wait::<u32>();
        drop(wait);
        assert!(!notify.notify(9).unwrap());
    }
}
