//! Compatibility registry for persistent and wire formats.
//!
//! Centralizes version ranges, magic values, and structured error reporting
//! for every format whose evolution must remain backward/forward compatible.
//!
//! Format contracts live under `doc/en/contract/` (English) and
//! `doc/cn/contract/` (Chinese).  When adding a new format family, register
//! its magic and supported version range here and update the corresponding
//! contract document before any code is released.

use crate::error::ErrorCode;
use crate::mudu_error;
use std::fmt;

/// Identifies a format family governed by the compatibility registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormatKind {
    /// On-disk page (full 4 KiB page block, see `doc/en/contract/page_zv1.md`).
    Page,
    /// Write-ahead log frame (see `doc/en/contract/log_frame_v1.md`).
    LogFrame,
    /// TCP wire protocol frame (see `doc/en/contract/protocol_frame_v1.md`).
    ProtocolFrame,
    /// MPK manifest (reserved; no loader exists yet).
    MpkManifest,
    /// Server configuration file (reserved; schema versioned via contract).
    ServerConfig,
    /// Overall file layout binding page files, WAL chunks, MPK packages and config
    /// (see `doc/en/contract/file_layout_v1.md`).
    FileLayout,
    /// Tuple binary format used inside pages, log entries and frames
    /// (see `doc/en/contract/tuple_binary_v1.md`).
    TupleBinary,
}

impl fmt::Display for FormatKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Page => f.write_str("page"),
            Self::LogFrame => f.write_str("log frame"),
            Self::ProtocolFrame => f.write_str("protocol frame"),
            Self::MpkManifest => f.write_str("mpk manifest"),
            Self::ServerConfig => f.write_str("server config"),
            Self::FileLayout => f.write_str("file layout"),
            Self::TupleBinary => f.write_str("tuple binary"),
        }
    }
}

/// Current version for the on-disk page format.
pub const PAGE_CURRENT_VERSION: u32 = 1;
/// Current version for the write-ahead log frame format.
pub const LOG_FRAME_CURRENT_VERSION: u32 = 1;
/// Current version for the TCP wire protocol frame format.
pub const PROTOCOL_FRAME_CURRENT_VERSION: u32 = 1;
/// Current version for the MPK manifest format.
pub const MPK_MANIFEST_CURRENT_VERSION: u32 = 1;
/// Current version for the server configuration format.
pub const SERVER_CONFIG_CURRENT_VERSION: u32 = 1;
/// Current version for the overall file layout format.
pub const FILE_LAYOUT_CURRENT_VERSION: u32 = 1;
/// Current version for the tuple binary format.
pub const TUPLE_BINARY_CURRENT_VERSION: u32 = 1;

/// Supported version range for a format family, inclusive on both ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionRange {
    /// Lowest supported version.
    pub min: u32,
    /// Highest supported version.
    pub max: u32,
}

impl VersionRange {
    /// Creates a new inclusive version range.
    pub const fn new(min: u32, max: u32) -> Self {
        Self { min, max }
    }

    /// Returns true if `version` falls inside this inclusive range.
    pub fn contains(&self, version: u32) -> bool {
        version >= self.min && version <= self.max
    }
}

impl fmt::Display for VersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}, {}]", self.min, self.max)
    }
}

/// Central registry of magic values and supported versions.
#[derive(Debug, Clone, Copy)]
pub struct CompatibilityMatrix;

impl CompatibilityMatrix {
    /// Registered magic value for `kind`.
    pub const fn magic(kind: FormatKind) -> u32 {
        match kind {
            FormatKind::Page => 0x5041_4745,          // PAGE
            FormatKind::LogFrame => 0x4C47_464D,      // LGFM
            FormatKind::ProtocolFrame => 0x4D53_464D, // MSFM
            FormatKind::MpkManifest => 0x4D50_4B4D,   // MPKM
            FormatKind::ServerConfig => 0,            // text config, no magic
            FormatKind::FileLayout => 0,              // composite format, no single magic
            FormatKind::TupleBinary => 0,             // schema-driven format, no single magic
        }
    }

    /// Supported version range for `kind`.
    pub const fn supported_versions(kind: FormatKind) -> VersionRange {
        match kind {
            FormatKind::Page => VersionRange::new(1, PAGE_CURRENT_VERSION),
            FormatKind::LogFrame => VersionRange::new(1, LOG_FRAME_CURRENT_VERSION),
            FormatKind::ProtocolFrame => VersionRange::new(1, PROTOCOL_FRAME_CURRENT_VERSION),
            FormatKind::MpkManifest => VersionRange::new(1, MPK_MANIFEST_CURRENT_VERSION),
            FormatKind::ServerConfig => VersionRange::new(1, SERVER_CONFIG_CURRENT_VERSION),
            FormatKind::FileLayout => VersionRange::new(1, FILE_LAYOUT_CURRENT_VERSION),
            FormatKind::TupleBinary => VersionRange::new(1, TUPLE_BINARY_CURRENT_VERSION),
        }
    }

    /// Latest supported version for `kind`.
    pub const fn latest_version(kind: FormatKind) -> u32 {
        match kind {
            FormatKind::Page => PAGE_CURRENT_VERSION,
            FormatKind::LogFrame => LOG_FRAME_CURRENT_VERSION,
            FormatKind::ProtocolFrame => PROTOCOL_FRAME_CURRENT_VERSION,
            FormatKind::MpkManifest => MPK_MANIFEST_CURRENT_VERSION,
            FormatKind::ServerConfig => SERVER_CONFIG_CURRENT_VERSION,
            FormatKind::FileLayout => FILE_LAYOUT_CURRENT_VERSION,
            FormatKind::TupleBinary => TUPLE_BINARY_CURRENT_VERSION,
        }
    }

    /// Returns true if `version` is supported for `kind`.
    pub fn is_supported(kind: FormatKind, version: u32) -> bool {
        Self::supported_versions(kind).contains(version)
    }
}

/// Structured compatibility failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatError {
    /// Magic value did not match the expected constant.
    MagicMismatch {
        /// Format family being decoded.
        kind: FormatKind,
        /// Expected magic value.
        expected: u32,
        /// Actual magic value read from the input.
        actual: u32,
    },
    /// Format version is outside the supported range.
    UnsupportedVersion {
        /// Format family being decoded.
        kind: FormatKind,
        /// Actual version read from the input.
        actual: u32,
        /// Supported version range.
        supported: VersionRange,
    },
    /// Payload is too short or structurally inconsistent.
    Corrupted {
        /// Format family being decoded.
        kind: FormatKind,
        /// Human-readable detail about the corruption.
        detail: String,
    },
}

impl fmt::Display for CompatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MagicMismatch {
                kind,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "invalid {kind} magic: expected {expected:#x}, got {actual:#x}"
                )
            }
            Self::UnsupportedVersion {
                kind,
                actual,
                supported,
            } => {
                write!(
                    f,
                    "unsupported {kind} version {actual}, supported range is {supported}"
                )
            }
            Self::Corrupted { kind, detail } => {
                write!(f, "corrupted {kind}: {detail}")
            }
        }
    }
}

impl std::error::Error for CompatError {}

impl CompatError {
    /// Maps the compatibility failure to a stable [`ErrorCode`].
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::MagicMismatch { .. } | Self::Corrupted { .. } => ErrorCode::CorruptedData,
            Self::UnsupportedVersion { kind, .. } => match kind {
                FormatKind::ProtocolFrame => ErrorCode::IncompatibleProtocolVersion,
                _ => ErrorCode::UnsupportedFormatVersion,
            },
        }
    }

    /// Converts this structured error into a [`crate::error::MuduError`].
    #[track_caller]
    pub fn into_mudu_error(self) -> crate::error::MuduError {
        let ec = self.error_code();
        mudu_error!(ec, self.to_string(), self)
    }
}

impl From<CompatError> for crate::error::MuduError {
    #[track_caller]
    fn from(err: CompatError) -> Self {
        err.into_mudu_error()
    }
}

/// Verifies that `actual` magic matches the registered magic for `kind`.
pub fn check_magic(kind: FormatKind, actual: u32) -> Result<(), CompatError> {
    let expected = CompatibilityMatrix::magic(kind);
    if actual == expected {
        Ok(())
    } else {
        Err(CompatError::MagicMismatch {
            kind,
            expected,
            actual,
        })
    }
}

/// Verifies that `version` is supported for `kind`.
pub fn check_version(kind: FormatKind, version: u32) -> Result<(), CompatError> {
    let supported = CompatibilityMatrix::supported_versions(kind);
    if supported.contains(version) {
        Ok(())
    } else {
        Err(CompatError::UnsupportedVersion {
            kind,
            actual: version,
            supported,
        })
    }
}

/// Verifies both magic and version for `kind`.
pub fn check_magic_and_version(
    kind: FormatKind,
    actual_magic: u32,
    version: u32,
) -> Result<(), CompatError> {
    check_magic(kind, actual_magic)?;
    check_version(kind, version)?;
    Ok(())
}

/// Returns a [`CompatError::Corrupted`] with the supplied detail.
pub fn corrupted(kind: FormatKind, detail: impl Into<String>) -> CompatError {
    CompatError::Corrupted {
        kind,
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_range_contains_boundaries() {
        let range = VersionRange::new(1, 3);
        assert!(!range.contains(0));
        assert!(range.contains(1));
        assert!(range.contains(2));
        assert!(range.contains(3));
        assert!(!range.contains(4));
    }

    #[test]
    fn matrix_rejects_unsupported_versions() {
        assert!(CompatibilityMatrix::is_supported(FormatKind::Page, 1));
        assert!(!CompatibilityMatrix::is_supported(FormatKind::Page, 2));
        assert!(!CompatibilityMatrix::is_supported(FormatKind::Page, 0));

        assert!(CompatibilityMatrix::is_supported(FormatKind::FileLayout, 1));
        assert!(!CompatibilityMatrix::is_supported(
            FormatKind::FileLayout,
            2
        ));

        assert!(CompatibilityMatrix::is_supported(
            FormatKind::TupleBinary,
            1
        ));
        assert!(!CompatibilityMatrix::is_supported(
            FormatKind::TupleBinary,
            2
        ));
    }

    #[test]
    fn check_magic_reports_mismatch() {
        let err = check_magic(FormatKind::LogFrame, 0xdead_beef).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::CorruptedData);
        assert!(matches!(
            err,
            CompatError::MagicMismatch {
                kind: FormatKind::LogFrame,
                expected: 0x4C47_464D,
                actual: 0xdead_beef,
            }
        ));
    }

    #[test]
    fn check_version_maps_protocol_to_distinct_code() {
        let err = check_version(FormatKind::ProtocolFrame, 99).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::IncompatibleProtocolVersion);

        let err = check_version(FormatKind::Page, 99).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::UnsupportedFormatVersion);
    }

    #[test]
    fn corrupted_error_round_trips_through_mudu_error() {
        let compat = corrupted(FormatKind::LogFrame, "truncated header");
        let err: crate::error::MuduError = compat.into();
        assert_eq!(err.ec(), ErrorCode::CorruptedData);
        assert!(err.message().contains("truncated header"));
    }

    #[test]
    fn all_format_kinds_have_consistent_metadata() {
        let cases = [
            (FormatKind::Page, 0x5041_4745, 1, "page"),
            (FormatKind::LogFrame, 0x4C47_464D, 1, "log frame"),
            (FormatKind::ProtocolFrame, 0x4D53_464D, 1, "protocol frame"),
            (FormatKind::MpkManifest, 0x4D50_4B4D, 1, "mpk manifest"),
            (FormatKind::ServerConfig, 0, 1, "server config"),
            (FormatKind::FileLayout, 0, 1, "file layout"),
            (FormatKind::TupleBinary, 0, 1, "tuple binary"),
        ];
        for (kind, magic, latest, display) in cases {
            assert_eq!(CompatibilityMatrix::magic(kind), magic);
            assert_eq!(CompatibilityMatrix::latest_version(kind), latest);
            assert_eq!(format!("{}", kind), display);
            assert!(CompatibilityMatrix::is_supported(kind, 1));
            assert!(!CompatibilityMatrix::is_supported(kind, latest + 1));
        }
    }

    #[test]
    fn version_range_display_shows_bounds() {
        let range = VersionRange::new(1, 5);
        assert_eq!(format!("{}", range), "[1, 5]");
    }

    #[test]
    fn compat_error_display_and_error_code() {
        let magic_err = CompatError::MagicMismatch {
            kind: FormatKind::Page,
            expected: 0x5041_4745,
            actual: 0,
        };
        assert_eq!(magic_err.error_code(), ErrorCode::CorruptedData);
        assert!(magic_err.to_string().contains("invalid page magic"));

        let version_err = CompatError::UnsupportedVersion {
            kind: FormatKind::ProtocolFrame,
            actual: 99,
            supported: VersionRange::new(1, 1),
        };
        assert_eq!(
            version_err.error_code(),
            ErrorCode::IncompatibleProtocolVersion
        );
        assert!(
            version_err
                .to_string()
                .contains("unsupported protocol frame version")
        );

        let corrupted_err = CompatError::Corrupted {
            kind: FormatKind::LogFrame,
            detail: "bad".to_string(),
        };
        assert_eq!(corrupted_err.error_code(), ErrorCode::CorruptedData);
        assert!(
            corrupted_err
                .to_string()
                .contains("corrupted log frame: bad")
        );
    }

    #[test]
    fn check_magic_and_version_accepts_valid_values() {
        assert!(check_magic(FormatKind::Page, 0x5041_4745).is_ok());
        assert!(check_version(FormatKind::Page, 1).is_ok());
        assert!(check_magic_and_version(FormatKind::Page, 0x5041_4745, 1).is_ok());
    }
}
