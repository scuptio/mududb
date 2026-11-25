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

#[cfg(test)]
mod tests {
    #[test]
    fn sys_process_default_and_new() {
        // `exit` terminates the process and is exercised only by real exits.
        #[allow(
            clippy::default_constructed_unit_structs,
            reason = "exercise the Default impl explicitly"
        )]
        let _ = super::SysProcess::default();
        let _ = super::SysProcess::new();
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn exit_preserves_status_in_child_process() {
        const MARKER: &str = "MUDU_SYS_PROCESS_EXIT_TEST";
        if std::env::var_os(MARKER).is_some() {
            super::SysProcess::new().exit(37);
        }
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "imp::native::process::tests::exit_preserves_status_in_child_process",
                "--nocapture",
            ])
            .env(MARKER, "1")
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(37), "child output: {:?}", result);
    }
}
