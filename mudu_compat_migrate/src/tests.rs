//! Tests for the migration router.

use crate::handler::{clone_rollback, clone_upgrade, MigrateHandler, MigrateOption};
use crate::router::{CompatibilityRouter, NoopOptionProvider, OptionProvider};
use crate::MigrateError;
use mudu::compat::{FormatKind, VersionRange};
use mudu::error::{ErrorCode, MuduError};

fn step_upgrade(old: &[u8], _option: Option<&MigrateOption>) -> Result<Vec<u8>, MigrateError> {
    if old.is_empty() {
        return Err(MigrateError::MigrationFailed {
            component: FormatKind::Page,
            from: 0,
            to: 0,
            step: 0,
            source: "empty input".to_string(),
        });
    }
    let mut out = old.to_vec();
    out[0] = out[0].saturating_add(1);
    Ok(out)
}

fn step_rollback(new: &[u8], _option: Option<&MigrateOption>) -> Result<Vec<u8>, MigrateError> {
    if new.is_empty() {
        return Err(MigrateError::MigrationFailed {
            component: FormatKind::Page,
            from: 0,
            to: 0,
            step: 0,
            source: "empty input".to_string(),
        });
    }
    let mut out = new.to_vec();
    out[0] = out[0].saturating_sub(1);
    Ok(out)
}

fn jump_upgrade(old: &[u8], _option: Option<&MigrateOption>) -> Result<Vec<u8>, MigrateError> {
    if old.is_empty() {
        return Err(MigrateError::MigrationFailed {
            component: FormatKind::Page,
            from: 0,
            to: 0,
            step: 0,
            source: "empty input".to_string(),
        });
    }
    let mut out = old.to_vec();
    out[0] = out[0].saturating_add(2);
    Ok(out)
}

fn jump_rollback(new: &[u8], _option: Option<&MigrateOption>) -> Result<Vec<u8>, MigrateError> {
    if new.is_empty() {
        return Err(MigrateError::MigrationFailed {
            component: FormatKind::Page,
            from: 0,
            to: 0,
            step: 0,
            source: "empty input".to_string(),
        });
    }
    let mut out = new.to_vec();
    out[0] = out[0].saturating_sub(2);
    Ok(out)
}

fn handler(from: u32, to: u32) -> MigrateHandler {
    MigrateHandler {
        from,
        to,
        upgrade: step_upgrade,
        rollback: step_rollback,
    }
}

fn jump_handler(from: u32, to: u32) -> MigrateHandler {
    MigrateHandler {
        from,
        to,
        upgrade: jump_upgrade,
        rollback: jump_rollback,
    }
}

#[test]
fn migrate_chain_upgrades_and_rollbacks() {
    let mut router = CompatibilityRouter::new();
    router.set_supported_window(FormatKind::Page, 1, 3);
    router.register(FormatKind::Page, handler(1, 2));
    router.register(FormatKind::Page, handler(2, 3));

    let input = vec![1u8, 42];
    let upgraded = router
        .upgrade_to_current(FormatKind::Page, 1, &input, &NoopOptionProvider)
        .unwrap();
    assert_eq!(upgraded, vec![3, 42]);

    let rolled_back = router
        .rollback_from_current(FormatKind::Page, 1, &upgraded, &NoopOptionProvider)
        .unwrap();
    assert_eq!(rolled_back, input);
}

#[test]
fn migrate_prefers_direct_handler_when_shorter() {
    let mut router = CompatibilityRouter::new();
    router.set_supported_window(FormatKind::Page, 1, 3);
    // Chain 1->2->3 exists.
    router.register(FormatKind::Page, handler(1, 2));
    router.register(FormatKind::Page, handler(2, 3));
    // Direct shortcut 1->3 also exists.
    router.register(FormatKind::Page, jump_handler(1, 3));

    let input = vec![1u8, 7];
    let upgraded = router
        .upgrade_to_current(FormatKind::Page, 1, &input, &NoopOptionProvider)
        .unwrap();
    assert_eq!(upgraded, vec![3, 7]);
}

#[test]
fn unsupported_version_outside_window() {
    let mut router = CompatibilityRouter::new();
    router.set_supported_window(FormatKind::LogFrame, 2, 3);
    router.register(FormatKind::LogFrame, handler(2, 3));

    let err = router
        .upgrade_to_current(FormatKind::LogFrame, 1, &[1], &NoopOptionProvider)
        .unwrap_err();
    assert!(matches!(
        err,
        MigrateError::UnsupportedVersion {
            component: FormatKind::LogFrame,
            actual: 1,
            ..
        }
    ));
}

#[test]
fn missing_handler_when_no_path_exists() {
    let mut router = CompatibilityRouter::new();
    router.set_supported_window(FormatKind::TupleBinary, 1, 3);
    router.register(FormatKind::TupleBinary, handler(1, 2));
    // No handler for 2->3.

    let err = router
        .upgrade_to_current(FormatKind::TupleBinary, 1, &[1], &NoopOptionProvider)
        .unwrap_err();
    assert!(matches!(
        err,
        MigrateError::MissingHandler {
            component: FormatKind::TupleBinary,
            from: 1,
            to: 3,
        }
    ));
}

struct VersionedOptionProvider;

impl OptionProvider for VersionedOptionProvider {
    fn get(&self, _component: FormatKind, version: u32) -> Option<MigrateOption> {
        Some(MigrateOption {
            version,
            payload: vec![version as u8],
        })
    }
}

fn option_aware_upgrade(
    old: &[u8],
    option: Option<&MigrateOption>,
) -> Result<Vec<u8>, MigrateError> {
    let mut out = old.to_vec();
    out[0] = out[0].saturating_add(1);
    if let Some(opt) = option {
        out.push(opt.version as u8);
        out.extend_from_slice(&opt.payload);
    } else {
        out.push(0);
    }
    Ok(out)
}

fn option_aware_rollback(
    new: &[u8],
    option: Option<&MigrateOption>,
) -> Result<Vec<u8>, MigrateError> {
    if new.len() < 3 {
        return Err(MigrateError::MigrationFailed {
            component: FormatKind::ProtocolFrame,
            from: 0,
            to: 0,
            step: 0,
            source: "too short".to_string(),
        });
    }
    let mut out = new.to_vec();
    // The upgrade appended exactly one version byte and one payload byte.
    out.truncate(out.len() - 2);
    if let Some(opt) = option {
        out.push(opt.version as u8);
    }
    out[0] = out[0].saturating_sub(1);
    Ok(out)
}

#[test]
fn option_provider_passes_versioned_payload() {
    let mut router = CompatibilityRouter::new();
    router.set_supported_window(FormatKind::ProtocolFrame, 1, 2);
    router.register(
        FormatKind::ProtocolFrame,
        MigrateHandler {
            from: 1,
            to: 2,
            upgrade: option_aware_upgrade,
            rollback: option_aware_rollback,
        },
    );

    let input = vec![1u8];
    let upgraded = router
        .upgrade_to_current(
            FormatKind::ProtocolFrame,
            1,
            &input,
            &VersionedOptionProvider,
        )
        .unwrap();
    // version byte becomes 2, then option trailer [version=1, payload=[1]] appended.
    assert_eq!(upgraded, vec![2, 1, 1]);

    let rolled_back = router
        .rollback_from_current(
            FormatKind::ProtocolFrame,
            1,
            &upgraded,
            &VersionedOptionProvider,
        )
        .unwrap();
    assert_eq!(rolled_back, vec![1, 2]);
}

#[test]
fn validate_path_reports_chain_length() {
    let mut router = CompatibilityRouter::new();
    router.set_supported_window(FormatKind::Page, 1, 3);
    router.register(FormatKind::Page, handler(1, 2));
    router.register(FormatKind::Page, handler(2, 3));

    assert_eq!(router.validate_path(FormatKind::Page, 1, 3).unwrap(), 2);
    assert_eq!(router.validate_path(FormatKind::Page, 2, 2).unwrap(), 0);
}

#[test]
fn migrate_error_display_messages() {
    let unsupported_version = MigrateError::UnsupportedVersion {
        component: FormatKind::Page,
        actual: 5,
        supported: VersionRange::new(1, 3),
    };
    assert_eq!(
        unsupported_version.to_string(),
        "unsupported page version 5, supported range is [1, 3]"
    );

    let missing_handler = MigrateError::MissingHandler {
        component: FormatKind::LogFrame,
        from: 1,
        to: 2,
    };
    assert_eq!(
        missing_handler.to_string(),
        "no migration handler registered for log frame from version 1 to 2"
    );

    let irreversible = MigrateError::Irreversible {
        component: FormatKind::TupleBinary,
        from: 2,
        to: 1,
        detail: "dropped column".to_string(),
    };
    assert_eq!(
        irreversible.to_string(),
        "cannot rollback tuple binary from version 2 to 1: dropped column"
    );

    let unsupported_option = MigrateError::UnsupportedOptionVersion {
        component: FormatKind::ProtocolFrame,
        version: 7,
    };
    assert_eq!(
        unsupported_option.to_string(),
        "unsupported option payload version 7 for protocol frame"
    );

    let migration_failed = MigrateError::MigrationFailed {
        component: FormatKind::FileLayout,
        from: 1,
        to: 2,
        step: 0,
        source: "bad header".to_string(),
    };
    assert_eq!(
        migration_failed.to_string(),
        "file layout migration step 0 (version 1 -> 2) failed: bad header"
    );
}

#[test]
fn migrate_error_error_code_mapping() {
    assert_eq!(
        MigrateError::UnsupportedVersion {
            component: FormatKind::ProtocolFrame,
            actual: 9,
            supported: VersionRange::new(1, 1),
        }
        .error_code(),
        ErrorCode::IncompatibleProtocolVersion
    );

    assert_eq!(
        MigrateError::UnsupportedVersion {
            component: FormatKind::Page,
            actual: 9,
            supported: VersionRange::new(1, 1),
        }
        .error_code(),
        ErrorCode::UnsupportedFormatVersion
    );

    assert_eq!(
        MigrateError::MissingHandler {
            component: FormatKind::Page,
            from: 1,
            to: 2,
        }
        .error_code(),
        ErrorCode::UnsupportedFormatVersion
    );

    assert_eq!(
        MigrateError::Irreversible {
            component: FormatKind::Page,
            from: 2,
            to: 1,
            detail: String::new(),
        }
        .error_code(),
        ErrorCode::UnsupportedFormatVersion
    );

    assert_eq!(
        MigrateError::UnsupportedOptionVersion {
            component: FormatKind::Page,
            version: 9,
        }
        .error_code(),
        ErrorCode::CorruptedData
    );

    assert_eq!(
        MigrateError::MigrationFailed {
            component: FormatKind::Page,
            from: 1,
            to: 2,
            step: 0,
            source: String::new(),
        }
        .error_code(),
        ErrorCode::CorruptedData
    );
}

#[test]
fn migrate_error_into_mudu_error() {
    let migrate = MigrateError::UnsupportedVersion {
        component: FormatKind::Page,
        actual: 5,
        supported: VersionRange::new(1, 1),
    };
    let err: MuduError = migrate.clone().into();
    assert_eq!(err.ec(), migrate.error_code());
    assert!(err.message().contains("unsupported page version"));
}

#[test]
fn clone_upgrade_and_rollback_are_identity() {
    let input = vec![1u8, 2, 3];
    let option = MigrateOption {
        version: 1,
        payload: vec![9],
    };

    assert_eq!(clone_upgrade(&input, Some(&option)).unwrap(), input);
    assert_eq!(clone_rollback(&input, Some(&option)).unwrap(), input);
    assert_eq!(clone_upgrade(&input, None).unwrap(), input);
    assert_eq!(clone_rollback(&input, None).unwrap(), input);
}

#[test]
fn migrate_handler_debug_shows_fn_placeholders() {
    let handler = MigrateHandler {
        from: 1,
        to: 2,
        upgrade: step_upgrade,
        rollback: step_rollback,
    };
    let debug = format!("{:?}", handler);
    assert!(debug.contains("from: 1"));
    assert!(debug.contains("to: 2"));
    assert!(debug.contains("upgrade: \"<fn>\""));
    assert!(debug.contains("rollback: \"<fn>\""));
}
