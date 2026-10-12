/// Process subsystem - native implementation.
pub struct SysProcess;

impl Default for SysProcess {
    fn default() -> Self {
        Self::new()
    }
}

impl SysProcess {
    /// Creates the process subsystem handle.
    pub fn new() -> Self {
        Self
    }

    /// Terminates the current process with exit code `code`.
    pub fn exit(&self, code: i32) -> ! {
        std::process::exit(code)
    }
}
