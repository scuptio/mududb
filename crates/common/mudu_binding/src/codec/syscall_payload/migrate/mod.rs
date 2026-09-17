//! Migration handlers for the guest→host syscall payload format.
//!
//! The module lives next to the syscall payload encode/decode implementation
//! so that the binding crate owns its own compatibility story.
//!
//! **Important:** migrate handlers for `FormatKind::SyscallPayload` operate on
//! the **complete frame** (16-byte MSSP header plus MessagePack body), not
//! just the header.  Only version 1 exists today, so the sole handler is the
//! `v1` identity handler; real `v1 -> v2` upgrade/rollback functions hook in
//! here when the syscall payload format evolves.

use mudu_compat_migrate::handler::{MigrateHandler, clone_rollback, clone_upgrade};

/// Returns a placeholder identity handler that clones the input frame
/// unchanged.
///
/// This will be replaced by real `v1 -> v2` handlers when the syscall payload
/// format evolves.  The input and output of the real handlers must be a
/// complete frame (header included) compatible with the target payload
/// version.
pub fn identity() -> MigrateHandler {
    MigrateHandler {
        from: 1,
        to: 1,
        upgrade: clone_upgrade,
        rollback: clone_rollback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mudu::compat::FormatKind;
    use mudu_compat_migrate::{CompatibilityRouter, MigrateError, NoopOptionProvider};

    #[test]
    fn identity_handler_clones_input() -> Result<(), MigrateError> {
        let mut router = CompatibilityRouter::new();
        router.set_supported_window(FormatKind::SyscallPayload, 1, 1);
        router.register(FormatKind::SyscallPayload, identity());
        let input = vec![0x4D, 0x53, 0x53, 0x50];
        let output = router.migrate(
            FormatKind::SyscallPayload,
            1,
            1,
            &input,
            &NoopOptionProvider,
        )?;
        assert_eq!(output, input);
        Ok(())
    }
}
