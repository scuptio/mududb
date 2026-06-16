pub struct IoUring;

impl IoUring {
    pub fn new(_entries: u32) -> Result<Self, i32> {
        Err(-1)
    }
}
