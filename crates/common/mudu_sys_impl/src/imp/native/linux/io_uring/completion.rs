/// A completed io_uring operation: the caller-provided `user_data` tag and
/// the syscall-style result (negative errno on failure).
#[derive(Clone, Copy, Debug)]
pub struct Completion {
    user_data: u64,
    result: i32,
}

impl Completion {
    pub(crate) fn new(user_data: u64, result: i32) -> Self {
        Self { user_data, result }
    }

    /// Returns the `user_data` tag set on the submission.
    pub fn user_data(&self) -> u64 {
        self.user_data
    }

    /// Returns the completion result: non-negative on success, negative
    /// errno on failure.
    pub fn result(&self) -> i32 {
        self.result
    }
}
