#![allow(missing_docs)]
/// Process subsystem - native implementation.
pub struct SysProcess;

impl Default for SysProcess {
    fn default() -> Self {
        Self::new()
    }
}

impl SysProcess {
    pub fn new() -> Self {
        Self
    }

    pub fn exit(&self, code: i32) -> ! {
        std::process::exit(code)
    }
}
