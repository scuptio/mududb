//! WAL archiving, base backup/PITR restore, and logical dump/restore.

#![warn(missing_docs)]

/// WAL archiving, base-backup orchestration, and the backup worker registry.
pub mod backup;
/// Full-database logical dump / restore (catalog metadata plus per-table CSV).
pub mod logical_backup;
/// Offline restore / point-in-time recovery from a base backup plus archived WAL.
pub mod restore;
