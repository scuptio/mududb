//! Kernel compatibility helpers.
//!
//! This module builds and installs the global [`CompatibilityRouter`] used by
//! format decode entry points across the workspace.

use mudu::compat::{CompatibilityMatrix, FormatKind};
use mudu_compat_migrate::CompatibilityRouter;

/// Builds a router that knows about every format owned by the kernel or the
/// contract crates, and installs it as the global compatibility router.
///
/// The function is idempotent: if a router has already been installed it
/// returns without doing anything.
pub fn install_compatibility_router() {
    if mudu_compat_migrate::global::is_installed() {
        return;
    }

    let mut router = CompatibilityRouter::new();

    let components = [
        FormatKind::Page,
        FormatKind::LogFrame,
        FormatKind::ProtocolFrame,
        FormatKind::TupleBinary,
    ];

    for component in components {
        let current = CompatibilityMatrix::latest_version(component);
        router.set_supported_window(component, 1, current);
    }

    router.register(FormatKind::Page, crate::storage::page::migrate::identity());
    router.register(FormatKind::LogFrame, crate::wal::migrate::identity());
    router.register(
        FormatKind::ProtocolFrame,
        mudu_contract::protocol::migrate::identity(),
    );
    router.register(
        FormatKind::TupleBinary,
        mudu_contract::tuple::migrate::identity(),
    );

    // Installation can only fail if something else already installed a router.
    // We checked above, so the result can be ignored.
    let _ = mudu_compat_migrate::global::install(router);
}
