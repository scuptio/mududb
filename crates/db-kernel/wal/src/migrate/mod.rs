//! Migration handlers for the write-ahead log frame format.
//!
//! The module lives next to the WAL frame encode/decode implementation so that
//! the WAL crate owns its own compatibility story.

use mudu_compat_migrate::handler::{clone_rollback, clone_upgrade, MigrateHandler};

/// Returns a placeholder identity handler that clones the input binary unchanged.
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
        router.set_supported_window(FormatKind::LogFrame, 1, 1);
        router.register(FormatKind::LogFrame, identity());
        let input = vec![0xABu8, 0xCD, 0xEF];
        let output = router.migrate(FormatKind::LogFrame, 1, 1, &input, &NoopOptionProvider)?;
        assert_eq!(output, input);
        Ok(())
    }
}
