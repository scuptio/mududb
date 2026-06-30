//! Migration handlers for the tuple binary format.
//!
//! The module lives next to the tuple encode/decode implementation so that the
//! tuple crate owns its own compatibility story.

use mudu_compat_migrate::handler::{MigrateHandler, clone_rollback, clone_upgrade};

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
        router.set_supported_window(FormatKind::TupleBinary, 1, 1);
        router.register(FormatKind::TupleBinary, identity());
        let input = vec![0xABu8, 0xCD, 0xEF];
        let output = router.migrate(FormatKind::TupleBinary, 1, 1, &input, &NoopOptionProvider)?;
        assert_eq!(output, input);
        Ok(())
    }
}
