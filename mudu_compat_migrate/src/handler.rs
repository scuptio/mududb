//! Migration handlers and option payloads.

use crate::error::MigrateError;
use std::fmt;

/// Versioned auxiliary payload passed to migration functions.
///
/// The `payload` structure depends on `version`; callers must encode the version
/// so the handler can pick the right decoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrateOption {
    /// Version of the payload layout.
    pub version: u32,
    /// Opaque auxiliary bytes.
    pub payload: Vec<u8>,
}

/// Upgrade function: transforms a binary blob from an older version to a newer
/// version.
///
/// The function must be deterministic and side-effect free.
pub type UpgradeFn =
    fn(old: &[u8], option: Option<&MigrateOption>) -> Result<Vec<u8>, MigrateError>;

/// Rollback function: transforms a binary blob from a newer version to an older
/// version.
///
/// The function must be deterministic and side-effect free.
pub type RollbackFn =
    fn(new: &[u8], option: Option<&MigrateOption>) -> Result<Vec<u8>, MigrateError>;

/// Pure upgrade function that simply clones its input.
///
/// Useful as a placeholder / identity handler for dummy migrations.
pub fn clone_upgrade(
    binary: &[u8],
    _option: Option<&MigrateOption>,
) -> Result<Vec<u8>, MigrateError> {
    Ok(binary.to_vec())
}

/// Pure rollback function that simply clones its input.
pub fn clone_rollback(
    binary: &[u8],
    _option: Option<&MigrateOption>,
) -> Result<Vec<u8>, MigrateError> {
    Ok(binary.to_vec())
}

/// A registered migration between two format versions.
#[derive(Clone)]
pub struct MigrateHandler {
    /// Source version.
    pub from: u32,
    /// Target version.
    pub to: u32,
    /// Upgrade implementation (`from -> to`).
    pub upgrade: UpgradeFn,
    /// Rollback implementation (`to -> from`).
    pub rollback: RollbackFn,
}

impl fmt::Debug for MigrateHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MigrateHandler")
            .field("from", &self.from)
            .field("to", &self.to)
            .field("upgrade", &"<fn>")
            .field("rollback", &"<fn>")
            .finish()
    }
}
