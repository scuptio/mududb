//! In-memory catalog managers for schemas, partitions, rules, and placements.

#![warn(missing_docs)]

#[cfg(any(test, feature = "test", fuzzing))]
/// Fuzz helpers generating schema-conformant rows (test/fuzz builds only).
pub mod _fuzz;

/// The `_fs_object` system table backing FS-bound columns.
pub mod fs_object;
/// Catalog of registered filesystem types.
pub mod fs_type_catalog;
/// The in-memory catalog manager over the system tables.
pub mod meta_mgr;
/// Factory building the in-memory catalog manager.
pub mod meta_mgr_factory;
/// Catalog of table partition bindings.
pub mod partition_binding_catalog;
/// Catalog of partition placements (partition-to-worker assignments).
pub mod partition_placement_catalog;
/// Catalog of partition rules.
pub mod partition_rule_catalog;
/// Catalog of table schemas.
pub mod schema_catalog;
