#![allow(clippy::unwrap_used)]

use super::{CompatibilityRouter, NoopOptionProvider, OptionProvider};
use crate::error::MigrateError;
use crate::handler::{clone_rollback, clone_upgrade, MigrateHandler, MigrateOption};
use mudu::compat::{FormatKind, VersionRange};

const COMPONENT: FormatKind = FormatKind::Page;

fn failing_upgrade(
    _binary: &[u8],
    _option: Option<&MigrateOption>,
) -> Result<Vec<u8>, MigrateError> {
    Err(MigrateError::Irreversible {
        component: COMPONENT,
        from: 1,
        to: 2,
        detail: "test".to_string(),
    })
}

fn increment_upgrade(
    binary: &[u8],
    _option: Option<&MigrateOption>,
) -> Result<Vec<u8>, MigrateError> {
    let mut v = binary.to_vec();
    v.push(binary[binary.len() - 1].wrapping_add(1));
    Ok(v)
}

fn decrement_rollback(
    binary: &[u8],
    _option: Option<&MigrateOption>,
) -> Result<Vec<u8>, MigrateError> {
    let mut v = binary.to_vec();
    v.pop();
    Ok(v)
}

struct TestOptionProvider;

impl OptionProvider for TestOptionProvider {
    fn get(&self, _component: FormatKind, version: u32) -> Option<MigrateOption> {
        Some(MigrateOption {
            version,
            payload: vec![version as u8],
        })
    }
}

fn router_with_chain() -> CompatibilityRouter {
    let mut router = CompatibilityRouter::new();
    router.set_supported_window(COMPONENT, 1, 3);
    router.register(
        COMPONENT,
        MigrateHandler {
            from: 1,
            to: 2,
            upgrade: increment_upgrade,
            rollback: decrement_rollback,
        },
    );
    router.register(
        COMPONENT,
        MigrateHandler {
            from: 2,
            to: 3,
            upgrade: clone_upgrade,
            rollback: clone_rollback,
        },
    );
    router
}

#[test]
fn version_setters_and_supported_window() {
    let mut router = CompatibilityRouter::new();
    assert!(router.supported_window(COMPONENT).is_none());

    router.set_current_version(COMPONENT, 5);
    assert_eq!(router.current_version(COMPONENT), Some(5));
    assert_eq!(
        router.supported_window(COMPONENT),
        Some(VersionRange::new(1, 5))
    );

    router.set_min_supported_version(COMPONENT, 3);
    assert_eq!(router.min_supported_version(COMPONENT), 3);
    assert_eq!(
        router.supported_window(COMPONENT),
        Some(VersionRange::new(3, 5))
    );

    router.set_supported_window(COMPONENT, 2, 7);
    assert_eq!(
        router.supported_window(COMPONENT),
        Some(VersionRange::new(2, 7))
    );
}

#[test]
fn same_version_returns_input_unchanged() {
    let router = router_with_chain();
    let input = b"hello";
    let out = router
        .migrate(COMPONENT, 2, 2, input, &NoopOptionProvider)
        .unwrap();
    assert_eq!(out, input);
}

#[test]
fn unsupported_versions_are_rejected() {
    let router = router_with_chain();

    let err = router
        .migrate(COMPONENT, 0, 2, b"x", &NoopOptionProvider)
        .unwrap_err();
    assert!(
        matches!(err, MigrateError::UnsupportedVersion { actual: 0, .. }),
        "got {err:?}"
    );

    let err = router
        .migrate(COMPONENT, 1, 5, b"x", &NoopOptionProvider)
        .unwrap_err();
    assert!(
        matches!(err, MigrateError::UnsupportedVersion { actual: 5, .. }),
        "got {err:?}"
    );
}

#[test]
fn upgrade_to_current_requires_current_version() {
    let mut router = CompatibilityRouter::new();
    router.set_min_supported_version(COMPONENT, 1);
    let err = router
        .upgrade_to_current(COMPONENT, 1, b"x", &NoopOptionProvider)
        .unwrap_err();
    assert!(
        matches!(
            err,
            MigrateError::UnsupportedVersion {
                actual: 1,
                supported: VersionRange { min: 0, max: 0 },
                ..
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn rollback_from_current_requires_current_version() {
    let mut router = CompatibilityRouter::new();
    router.set_min_supported_version(COMPONENT, 1);
    let err = router
        .rollback_from_current(COMPONENT, 1, b"x", &NoopOptionProvider)
        .unwrap_err();
    assert!(
        matches!(
            err,
            MigrateError::UnsupportedVersion {
                actual: 1,
                supported: VersionRange { min: 0, max: 0 },
                ..
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn migration_chains_and_options() {
    let router = router_with_chain();
    let input = b"ab";

    let out = router
        .migrate(COMPONENT, 1, 3, input, &TestOptionProvider)
        .unwrap();
    assert_eq!(out, b"abc");

    let back = router
        .migrate(COMPONENT, 3, 1, &out, &NoopOptionProvider)
        .unwrap();
    assert_eq!(back, input);
}

#[test]
fn missing_handler_returns_error() {
    let mut router = CompatibilityRouter::new();
    router.set_supported_window(COMPONENT, 1, 5);
    router.register(
        COMPONENT,
        MigrateHandler {
            from: 1,
            to: 2,
            upgrade: clone_upgrade,
            rollback: clone_rollback,
        },
    );

    let err = router
        .migrate(COMPONENT, 1, 3, b"x", &NoopOptionProvider)
        .unwrap_err();
    assert!(
        matches!(
            err,
            MigrateError::MissingHandler {
                component: FormatKind::Page,
                from: 1,
                to: 3,
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn handler_failure_is_wrapped_as_migration_failed() {
    let mut router = CompatibilityRouter::new();
    router.set_supported_window(COMPONENT, 1, 2);
    router.register(
        COMPONENT,
        MigrateHandler {
            from: 1,
            to: 2,
            upgrade: failing_upgrade,
            rollback: clone_rollback,
        },
    );

    let err = router
        .migrate(COMPONENT, 1, 2, b"x", &NoopOptionProvider)
        .unwrap_err();
    assert!(
        matches!(
            err,
            MigrateError::MigrationFailed {
                component: FormatKind::Page,
                from: 1,
                to: 2,
                step: 0,
                ..
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn validate_path_counts_steps() {
    let mut router = router_with_chain();
    assert_eq!(router.validate_path(COMPONENT, 2, 2).unwrap(), 0);
    assert_eq!(router.validate_path(COMPONENT, 1, 3).unwrap(), 2);

    router.set_current_version(COMPONENT, 4);
    let err = router.validate_path(COMPONENT, 1, 4).unwrap_err();
    assert!(matches!(err, MigrateError::MissingHandler { .. }));
}

#[test]
fn edge_debug_format() {
    use super::EdgeKind;
    let edge = super::Edge {
        from: 1,
        to: 2,
        kind: EdgeKind::Upgrade,
        handler: MigrateHandler {
            from: 1,
            to: 2,
            upgrade: clone_upgrade,
            rollback: clone_rollback,
        },
    };
    let s = format!("{:?}", edge);
    assert!(s.contains("from: 1"));
    assert!(s.contains("to: 2"));
    assert!(s.contains("Upgrade"));
}
