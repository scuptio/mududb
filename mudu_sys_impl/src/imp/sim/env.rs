use crate::io::fd::RawFd;
use crate::sync::async_::Notifier;
use crate::sync::blocking::{ChannelReceiver, ChannelSender, ChannelSyncSender};
use crate::sync::std_mutex::SMutex;
use crate::sync::std_rwlock::SRwLock;
use crate::task::sync::SJoinHandle;
use chrono::{DateTime, Utc};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::future::Future;
use std::time::{Duration, Instant, SystemTime};
use tokio::task::JoinHandle;
use uuid::Uuid;

/// System environment aggregate - sim implementation.
pub struct Sys;

impl Default for Sys {
    fn default() -> Self {
        Self::new()
    }
}

impl Sys {
    pub const fn new() -> Self {
        Self
    }

    // ===== Associated functions: stateful objects =====
    pub fn fs() -> super::fs::Fs {
        super::fs::Fs
    }

    pub fn fs_sync() -> super::fs_sync::FsSync {
        super::fs_sync::FsSync::new()
    }

    pub fn net() -> super::net::Net {
        super::net::Net
    }

    pub fn runtime() -> Option<super::runtime::Runtime> {
        None
    }

    // ===== Associated functions: stateless operations =====
    pub fn instant_now() -> Instant {
        super::time::Time::instant_now()
    }

    pub fn system_time_now() -> SystemTime {
        super::time::Time::system_time_now()
    }

    pub fn utc_now() -> DateTime<Utc> {
        super::time::Time::utc_now()
    }

    pub fn uuid_v4() -> Uuid {
        super::random::Random::uuid_v4()
    }

    pub fn var(key: &str) -> Option<String> {
        super::env_var::EnvVar::var(key)
    }

    pub fn var_os(key: &str) -> Option<std::ffi::OsString> {
        super::env_var::EnvVar::var_os(key)
    }

    pub fn set_var(key: &str, value: &str) {
        super::env_var::EnvVar::set_var(key, value);
    }

    pub fn remove_var(key: &str) {
        super::env_var::EnvVar::remove_var(key);
    }

    pub fn temp_dir() -> std::path::PathBuf {
        super::env_var::EnvVar::temp_dir()
    }

    pub fn current_dir() -> RS<std::path::PathBuf> {
        super::env_var::EnvVar::current_dir()
    }

    pub fn args_os() -> Vec<std::ffi::OsString> {
        super::env_var::EnvVar::args_os()
    }

    pub fn eventfd() -> RS<RawFd> {
        super::sync::Sync::eventfd()
    }

    pub fn notify_eventfd(fd: RawFd) -> RS<()> {
        super::sync::Sync::notify_eventfd(fd)
    }

    pub fn read_eventfd(fd: RawFd) -> RS<u64> {
        super::sync::Sync::read_eventfd(fd)
    }

    pub fn close_fd(fd: RawFd) -> RS<()> {
        super::sync::Sync::close_fd(fd)
    }

    pub fn exit(_code: i32) -> ! {
        panic!("[sim] Sys::exit not implemented")
    }

    pub async fn sleep(dur: Duration) -> RS<()> {
        super::task_async::TaskAsync::sleep(dur).await
    }

    pub async fn timeout<F>(dur: Duration, fut: F) -> Option<F::Output>
    where
        F: std::future::Future,
    {
        super::task_async::TaskAsync::timeout(dur, fut).await
    }

    pub fn sleep_blocking(dur: Duration) {
        super::task_sync::TaskSync::sleep_blocking(dur)
    }

    // ===== Factory methods: sync primitives =====
    pub fn mutex<T>(value: T) -> SMutex<T> {
        SMutex::new(value)
    }

    pub fn rwlock<T>(value: T) -> SRwLock<T> {
        SRwLock::new(value)
    }

    pub fn channel<T>() -> (ChannelSender<T>, ChannelReceiver<T>) {
        super::channel::channel()
    }

    pub fn sync_channel<T>(bound: usize) -> (ChannelSyncSender<T>, ChannelReceiver<T>) {
        super::channel::sync_channel(bound)
    }

    // ===== Factory methods: io_uring =====
    pub fn io_uring(_entries: u32) -> RS<super::io_uring::IoUring> {
        Err(m_error!(EC::NotImplemented, "[sim] Sys::io_uring"))
    }

    // ===== Factory methods: thread / task =====
    pub fn spawn_thread<F, T>(f: F) -> RS<SJoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        super::thread::spawn_thread(f)
    }

    pub fn spawn_thread_named<F, T>(name: impl Into<String>, f: F) -> RS<SJoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        super::thread::spawn_thread_named(name, f)
    }

    pub fn spawn_task_with_waiter<F>(
        cancel_waiter: crate::sync::async_::Waiter,
        name: &str,
        f: F,
    ) -> RS<JoinHandle<Option<F::Output>>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        super::spawn::spawn_task(cancel_waiter, name, f)
    }

    pub fn spawn_task<F>(name: &str, f: F) -> RS<JoinHandle<Option<F::Output>>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (_cancel_notifier, cancel_waiter) = crate::sync::async_::notify_wait();
        Self::spawn_task_with_waiter(cancel_waiter, name, f)
    }

    pub async fn spawn_blocking<F, R>(f: F) -> RS<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        super::spawn::spawn_blocking(f).await
    }

    pub fn spawn_local_task_with_waiter<F>(
        cancel_waiter: crate::sync::async_::Waiter,
        name: &str,
        f: F,
    ) -> RS<JoinHandle<Option<F::Output>>>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        super::spawn_local::spawn_local_task(cancel_waiter, name, f)
    }

    pub fn spawn_local_task<F>(name: &str, f: F) -> RS<JoinHandle<Option<F::Output>>>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let (_cancel_notifier, cancel_waiter) = crate::sync::async_::notify_wait();
        Self::spawn_local_task_with_waiter(cancel_waiter, name, f)
    }

    pub fn build_current_thread_runtime() -> RS<tokio::runtime::Runtime> {
        super::task_runtime::build_current_thread_runtime()
    }

    pub fn build_multi_thread_runtime() -> RS<tokio::runtime::Runtime> {
        super::task_runtime::build_multi_thread_runtime()
    }

    pub fn wait_for_shutdown_signal(stop: Notifier) {
        super::task_runtime::wait_for_shutdown_signal(stop)
    }

    pub fn has_tokio_runtime() -> bool {
        super::task_runtime::has_tokio_runtime()
    }

    // ===== Factory methods: async sync primitives =====
    pub fn async_mutex<T>(value: T) -> crate::sync::async_::AMutex<T> {
        crate::sync::async_::AMutex::new(value)
    }

    pub fn async_rwlock<T>(value: T) -> crate::sync::async_::ARwLock<T> {
        crate::sync::async_::ARwLock::new(value)
    }

    pub fn async_notify() -> crate::sync::async_::ANotify {
        crate::sync::async_::ANotify::new()
    }

    pub fn stop_channel() -> (crate::sync::async_::StopTx, crate::sync::async_::StopRx) {
        crate::sync::async_::stop_channel()
    }

    pub fn async_notify_wait() -> (crate::sync::async_::Notifier, crate::sync::async_::Waiter) {
        crate::sync::async_::notify_wait()
    }

    pub fn futures_mutex<T>(value: T) -> crate::sync::async_::FMutex<T> {
        crate::sync::async_::FMutex::new(value)
    }
}
