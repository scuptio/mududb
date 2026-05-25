use crate::env::default_env;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::thread;
use std::time::Duration;

pub fn sleep_blocking(dur: Duration) {
    default_env().task_sync().sleep_blocking(dur)
}

pub fn spawn_thread<F, T>(f: F) -> RS<thread::JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    Ok(thread::spawn(f))
}

pub fn spawn_thread_named<F, T>(name: impl Into<String>, f: F) -> RS<thread::JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    thread::Builder::new()
        .name(name.into())
        .spawn(f)
        .map_err(|e| m_error!(EC::ThreadErr, "spawn thread error", e))
}
