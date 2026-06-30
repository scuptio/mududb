#[derive(Clone, Copy, Debug)]
pub struct Completion {
    user_data: u64,
    result: i32,
}

impl Completion {
    pub(crate) fn new(user_data: u64, result: i32) -> Self {
        Self { user_data, result }
    }

    pub fn user_data(&self) -> u64 {
        self.user_data
    }

    pub fn result(&self) -> i32 {
        self.result
    }
}
