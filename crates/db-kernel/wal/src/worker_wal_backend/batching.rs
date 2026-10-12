use std::time::Duration;

// The adaptive flush batching path is driven by the io_uring worker-ring event
// loop. Tokio callers use the direct async path, so these private fields are
// intentionally quiet there while still being checked on io_uring builds.
/// Flush-batching watermarks of a worker log: when a group-commit flush round
/// starts and how much it may coalesce.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone, Copy, Debug)]
pub struct WorkerLogBatching {
    /// Queued payload bytes that trigger a flush round.
    pub trigger_bytes: usize,
    /// Queued frame count that triggers a flush round.
    pub trigger_frames: usize,
    /// Age of the oldest queued batch that triggers a flush round on its own.
    pub max_wait: Duration,
    /// Upper bound on the payload bytes one flush round drains.
    pub max_batch_bytes: usize,
    /// Active sessions per concurrency-scaling step of the triggers.
    pub sessions_per_step: usize,
    /// Trigger-bytes increment per scaling step.
    pub bytes_per_step: usize,
    /// Trigger-frames increment per scaling step.
    pub frames_per_step: usize,
    /// Cap on the concurrency-scaled byte trigger.
    pub max_trigger_bytes: usize,
    /// Cap on the concurrency-scaled frame trigger.
    pub max_trigger_frames: usize,
}

impl WorkerLogBatching {
    /// Creates watermarks with the base triggers and default
    /// concurrency-scaling parameters.
    pub const fn new(
        trigger_bytes: usize,
        trigger_frames: usize,
        max_wait: Duration,
        max_batch_bytes: usize,
    ) -> Self {
        Self {
            trigger_bytes,
            trigger_frames,
            max_wait,
            max_batch_bytes,
            sessions_per_step: 8,
            bytes_per_step: 32 * 1024,
            frames_per_step: 16,
            max_trigger_bytes: 512 * 1024,
            max_trigger_frames: 256,
        }
    }

    /// Overrides the concurrency-scaling parameters.
    pub const fn with_session_scaling(
        mut self,
        sessions_per_step: usize,
        bytes_per_step: usize,
        frames_per_step: usize,
        max_trigger_bytes: usize,
        max_trigger_frames: usize,
    ) -> Self {
        self.sessions_per_step = sessions_per_step;
        self.bytes_per_step = bytes_per_step;
        self.frames_per_step = frames_per_step;
        self.max_trigger_bytes = max_trigger_bytes;
        self.max_trigger_frames = max_trigger_frames;
        self
    }
}

impl Default for WorkerLogBatching {
    fn default() -> Self {
        Self::new(64 * 1024, 32, Duration::from_micros(200), 256 * 1024)
    }
}
