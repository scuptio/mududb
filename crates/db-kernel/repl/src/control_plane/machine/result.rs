//! Result waiters observe success, apply failure, driver exit and shutdown.
use super::*;
use mudu_sys::tokio::sync::watch;

pub(crate) struct FailureGuard<'a> {
    failed: &'a watch::Sender<bool>,
    completed: bool,
}
impl FailureGuard<'_> {
    pub(crate) fn complete(mut self) {
        self.completed = true;
    }
}
impl Drop for FailureGuard<'_> {
    fn drop(&mut self) {
        if !self.completed {
            self.failed.send_replace(true);
        }
    }
}

impl MetaStreamMachine {
    /// An unfinished apply or exited driver wakes all pending result waiters,
    /// including when its future is dropped while suspended.
    pub(in crate::control_plane) fn failure_guard(&self) -> FailureGuard<'_> {
        FailureGuard {
            failed: &self.failed,
            completed: false,
        }
    }

    pub(in crate::control_plane) fn ensure_available(&self) -> RS<()> {
        if *self.failed.borrow() {
            return Err(invalid(
                "metadata apply is unavailable; reopen before retrying",
            ));
        }
        Ok(())
    }

    pub(in crate::control_plane) async fn wait_request(
        &self,
        request_id: u128,
        mut stopping: watch::Receiver<bool>,
    ) -> RS<()> {
        let mut failed = self.failed.subscribe();
        loop {
            // Construct before checking the predicate: notify_waiters issued
            // between the check and await must not be lost.
            let applied = self.applied_notify().notified();
            if self.request_applied(request_id)? {
                return Ok(());
            }
            if *stopping.borrow() {
                return Err(invalid(
                    "control service stopped before local metadata apply",
                ));
            }
            self.ensure_available()?;
            mudu_sys::tokio::select! {
                _ = applied => (),
                changed = failed.changed() => {
                    if changed.is_err() { return Err(invalid("metadata apply observer closed")); }
                },
                changed = stopping.changed() => {
                    if changed.is_err() { return Err(invalid("control service shutdown observer closed")); }
                },
            }
        }
    }
}
