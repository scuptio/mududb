//! Errors produced by the format/protocol migration machinery.

use mudu::compat::{FormatKind, VersionRange};
use mudu::error::{ErrorCode, MuduError};
use mudu::mudu_error;
use std::fmt;

/// Failure that can occur while planning or executing a migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrateError {
    /// The requested version is outside the supported window for this component.
    UnsupportedVersion {
        /// Component being migrated.
        component: FormatKind,
        /// Actual version encountered.
        actual: u32,
        /// Supported inclusive version range.
        supported: VersionRange,
    },
    /// No registered handler can move between the two requested versions.
    MissingHandler {
        /// Component being migrated.
        component: FormatKind,
        /// Source version.
        from: u32,
        /// Target version.
        to: u32,
    },
    /// The migration is not reversible, usually because a field was dropped.
    Irreversible {
        /// Component being migrated.
        component: FormatKind,
        /// Source version.
        from: u32,
        /// Target version.
        to: u32,
        /// Human-readable explanation.
        detail: String,
    },
    /// The optional auxiliary payload has a version the handler does not understand.
    UnsupportedOptionVersion {
        /// Component being migrated.
        component: FormatKind,
        /// Option payload version.
        version: u32,
    },
    /// A single migration step failed.
    MigrationFailed {
        /// Component being migrated.
        component: FormatKind,
        /// Source version of the failing step.
        from: u32,
        /// Target version of the failing step.
        to: u32,
        /// Zero-based index of the failing step in the migration chain.
        step: usize,
        /// Human-readable reason.
        source: String,
    },
}

impl fmt::Display for MigrateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion {
                component,
                actual,
                supported,
            } => write!(
                f,
                "unsupported {component} version {actual}, supported range is {supported}"
            ),
            Self::MissingHandler {
                component,
                from,
                to,
            } => write!(
                f,
                "no migration handler registered for {component} from version {from} to {to}"
            ),
            Self::Irreversible {
                component,
                from,
                to,
                detail,
            } => write!(
                f,
                "cannot rollback {component} from version {from} to {to}: {detail}"
            ),
            Self::UnsupportedOptionVersion { component, version } => write!(
                f,
                "unsupported option payload version {version} for {component}"
            ),
            Self::MigrationFailed {
                component,
                from,
                to,
                step,
                source,
            } => write!(
                f,
                "{component} migration step {step} (version {from} -> {to}) failed: {source}"
            ),
        }
    }
}

impl std::error::Error for MigrateError {}

impl MigrateError {
    /// Maps the migration failure to a stable [`ErrorCode`].
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::UnsupportedVersion { component, .. } => match component {
                FormatKind::ProtocolFrame => ErrorCode::IncompatibleProtocolVersion,
                _ => ErrorCode::UnsupportedFormatVersion,
            },
            Self::MissingHandler { .. } | Self::Irreversible { .. } => {
                ErrorCode::UnsupportedFormatVersion
            }
            Self::UnsupportedOptionVersion { .. } | Self::MigrationFailed { .. } => {
                ErrorCode::CorruptedData
            }
        }
    }

    /// Converts this structured error into a [`MuduError`].
    #[track_caller]
    pub fn into_mudu_error(self) -> MuduError {
        let ec = self.error_code();
        mudu_error!(ec, self.to_string(), self)
    }
}

impl From<MigrateError> for MuduError {
    #[track_caller]
    fn from(err: MigrateError) -> Self {
        err.into_mudu_error()
    }
}
