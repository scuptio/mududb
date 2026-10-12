//! Kernel-facing contracts and data types exchanged between subsystems.
//!
//! This module defines the value types, catalog descriptors, command/query
//! execution contracts, snapshot metadata, and version-tracking structures
//! used by the storage, WAL, server, and SQL layers.

/// Log sequence number type.
pub mod lsn;
/// Tests for [`lsn`].
#[cfg(test)]
pub mod lsn_test;
/// Transaction key-lock manager contract.
pub mod x_lock_mgr;

pub mod app_schema;
/// Worker-local message bus API: envelopes, filters, and the endpoint registry.
pub mod message_bus_api;
/// Catalog metadata manager trait (tables, partition rules, fs types).
pub mod meta_mgr;
/// Tests for [`meta_mgr`].
#[cfg(test)]
pub mod meta_mgr_test;
/// Partition rule descriptors (range partitions).
pub mod partition_rule;
/// Table-to-partition-rule bindings and worker placements.
pub mod partition_rule_binding;
/// Tests for [`partition_rule_binding`].
#[cfg(test)]
pub mod partition_rule_binding_test;
/// Tests for [`partition_rule`].
#[cfg(test)]
pub mod partition_rule_test;

/// Command execution contract (prepare/run/affected rows).
pub mod cmd_exec;
/// Multi-versioned in-memory row with a delta chain.
pub mod data_row;
mod field_info;
/// Tests for `field_info`.
#[cfg(test)]
pub mod field_info_test;
/// Filesystem-type catalog descriptors.
pub mod fs_type;
/// Query execution contract (open/next/tuple descriptor).
pub mod query_exec;
/// Column schema descriptor.
pub mod schema_column;
/// Tests for [`schema_column`].
#[cfg(test)]
pub mod schema_column_test;
/// Table schema descriptor and tuple-descriptor derivation.
pub mod schema_table;
/// Tests for [`schema_table`].
#[cfg(test)]
pub mod schema_table_test;
/// MVCC snapshot visibility types.
pub mod snapshot;
/// Session context contract.
pub mod ssn_ctx;
/// Immutable runtime table descriptor.
pub mod table_desc;
/// Tests for [`table_desc`].
#[cfg(test)]
pub mod table_desc_test;
/// Table metadata building immutable [`table_desc::TableDesc`] snapshots.
pub mod table_info;
/// Tests for [`table_info`].
#[cfg(test)]
pub mod table_info_test;
mod test_schema;
/// Per-worker snapshot and xid allocation.
pub mod worker_snapshot;
#[cfg(any(test, feature = "test", fuzzing))]
pub use self::test_schema::_fuzz::_schema_table;

/// Commit timestamp range of a tuple version.
pub mod timestamp;
/// Tests for [`timestamp`].
#[cfg(test)]
pub mod timestamp_test;
/// Delta between consecutive tuple versions.
pub mod version_delta;
/// A single tuple version with its commit timestamp.
pub mod version_tuple;
/// Async waiter contract.
pub mod waiter;
/// Fuzz targets for update-delta encode/decode.
pub mod xl_d_up_tuple;
