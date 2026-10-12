//! Migration handlers for the on-disk page format.
//!
//! The module lives next to the page encode/decode implementation so that the
//! page format crate owns its own compatibility story.
//!
//! **Important:** migrate handlers for `FormatKind::Page` operate on the
//! **complete page binary** (e.g. the full 4 KiB page block), not just the
//! 128-byte page header.  When a page format evolves, fields outside the header
//! (slot array, tailer, record payloads) may also change, so the migration unit
//! must be the whole page.

use mudu_compat_migrate::handler::{clone_rollback, clone_upgrade, MigrateHandler};

/// Returns a placeholder identity handler that clones the input page unchanged.
///
/// This will be replaced by real `v1 -> v2` handlers when the page format
/// evolves.  The input and output of the real handlers must be a complete page
/// binary compatible with the target page version.
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
        router.set_supported_window(FormatKind::Page, 1, 1);
        router.register(FormatKind::Page, identity());
        let input = vec![0xABu8, 0xCD, 0xEF];
        let output = router.migrate(FormatKind::Page, 1, 1, &input, &NoopOptionProvider)?;
        assert_eq!(output, input);
        Ok(())
    }
}
