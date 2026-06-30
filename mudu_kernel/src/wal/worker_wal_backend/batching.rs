use std::time::Duration;

// The adaptive flush batching path is driven by the io_uring worker-ring event
// loop. Tokio callers use the direct async path, so these private fields are
// intentionally quiet there while still being checked on io_uring builds.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone, Copy, Debug)]
pub struct WorkerLogBatching {
    pub(crate) trigger_bytes: usize,
    pub(crate) trigger_frames: usize,
    pub(crate) max_wait: Duration,
    pub(crate) max_batch_bytes: usize,
    pub(crate) sessions_per_step: usize,
    pub(crate) bytes_per_step: usize,
    pub(crate) frames_per_step: usize,
    pub(crate) max_trigger_bytes: usize,
    pub(crate) max_trigger_frames: usize,
}

impl WorkerLogBatching {
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
