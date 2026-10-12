use crate::cli::WorkerPhase;
use mudu_utils::debug::debug_serve;
use mudu_utils::notifier::NotifyWait;
use mududb::common::result::RS;
use mududb::sys::task::sync::{SJoinHandle, spawn_thread_named};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const PROGRESS_REPORT_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) struct DebugHttpServer {
    port: u16,
    canceler: NotifyWait,
    handle: Option<SJoinHandle<()>>,
}

pub(crate) struct ProgressTracker {
    started: AtomicUsize,
    completed: AtomicUsize,
}

pub(crate) struct ProgressReporter {
    total: usize,
    tracker: Arc<ProgressTracker>,
    handle: Option<SJoinHandle<()>>,
}

impl DebugHttpServer {
    pub(crate) fn start(port: Option<u16>) -> RS<Option<Self>> {
        let Some(port) = port else {
            return Ok(None);
        };
        let canceler = NotifyWait::new_with_name("ycsb-debug-http".to_string());
        let thread_canceler = canceler.clone();
        let handle = spawn_thread_named("ycsb-debug-http".to_string(), move || {
            debug_serve(thread_canceler, port)
        })
        .map_err(|e| {
            mududb::mudu_error!(
                mududb::error::ErrorCode::Thread,
                "spawn ycsb debug http server error",
                e
            )
        })?;
        eprintln!("ycsb benchmark debug_http_addr=0.0.0.0:{port}");
        Ok(Some(Self {
            port,
            canceler,
            handle: Some(handle),
        }))
    }
}

impl ProgressReporter {
    pub(crate) fn start(phase: WorkerPhase, total: usize) -> Option<Self> {
        if total == 0 {
            return None;
        }
        let tracker = Arc::new(ProgressTracker {
            started: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
        });
        let thread_tracker = Arc::clone(&tracker);
        let handle = spawn_thread_named(
            format!("ycsb-progress-{}", phase.as_str()),
            move || {
                let start = mududb::sys::time::instant_now();
                let mut last_completed = 0;
                let mut last_report = start;
                loop {
                    mududb::sys::sleep_blocking(PROGRESS_REPORT_INTERVAL);
                    let started = thread_tracker.started.load(Ordering::Relaxed).min(total);
                    let completed = thread_tracker.completed.load(Ordering::Relaxed).min(total);
                    let now = mududb::sys::time::instant_now();
                    let delta = completed.saturating_sub(last_completed);
                    let delta_secs = now.duration_since(last_report).as_secs_f64().max(0.000_001);
                    let total_secs = now.duration_since(start).as_secs_f64().max(0.000_001);
                    eprintln!(
                        "ycsb progress phase={} started={}/{} {:.2}% completed={}/{} {:.2}% rate={:.2}/s avg_rate={:.2}/s elapsed={:.1}s",
                        phase.as_str(),
                        started,
                        total,
                        started as f64 * 100.0 / total as f64,
                        completed,
                        total,
                        completed as f64 * 100.0 / total as f64,
                        delta as f64 / delta_secs,
                        completed as f64 / total_secs,
                        total_secs,
                    );
                    last_completed = completed;
                    last_report = now;
                    if completed >= total {
                        break;
                    }
                }
            },
        )
        .ok()?;
        Some(Self {
            total,
            tracker,
            handle: Some(handle),
        })
    }

    pub(crate) fn tracker(&self) -> Arc<ProgressTracker> {
        Arc::clone(&self.tracker)
    }

    pub(crate) fn finish(mut self) {
        self.tracker.started.store(self.total, Ordering::Relaxed);
        self.tracker.completed.store(self.total, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for DebugHttpServer {
    fn drop(&mut self) {
        self.canceler.notify_all();
        if let Some(handle) = self.handle.take()
            && handle.join().is_err()
        {
            eprintln!(
                "ycsb benchmark debug http server thread panicked: port={}",
                self.port
            );
        }
    }
}

pub(crate) fn mark_progress_started(progress: Option<&ProgressTracker>, started: usize) {
    if let Some(progress) = progress {
        progress.started.fetch_add(started, Ordering::Relaxed);
    }
}

pub(crate) fn mark_progress_completed(progress: Option<&ProgressTracker>, completed: usize) {
    if let Some(progress) = progress {
        progress.completed.fetch_add(completed, Ordering::Relaxed);
    }
}
