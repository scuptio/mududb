//! Kernel-facing contracts and data types exchanged between subsystems.
//!
//! This module defines the value types, catalog descriptors, command/query
//! execution contracts, snapshot metadata, and version-tracking structures
//! used by the storage, WAL, server, and SQL layers.

#![allow(missing_docs)]

pub mod lsn;
#[cfg(test)]
pub mod lsn_test;
pub mod x_lock_mgr;

pub mod meta_mgr;
#[cfg(test)]
pub mod meta_mgr_test;
pub mod partition_rule;
pub mod partition_rule_binding;
#[cfg(test)]
pub mod partition_rule_binding_test;
#[cfg(test)]
pub mod partition_rule_test;

pub mod cmd_exec;
pub mod data_row;
mod field_info;
#[cfg(test)]
pub mod field_info_test;
pub mod query_exec;
pub mod schema_column;
#[cfg(test)]
pub mod schema_column_test;
pub mod schema_table;
#[cfg(test)]
pub mod schema_table_test;
pub mod snapshot;
pub mod ssn_ctx;
pub mod table_desc;
#[cfg(test)]
pub mod table_desc_test;
pub mod table_info;
#[cfg(test)]
pub mod table_info_test;
mod test_schema;
#[cfg(any(test, fuzzing))]
pub use self::test_schema::_fuzz::_schema_table;

pub mod timestamp;
#[cfg(test)]
pub mod timestamp_test;
pub mod version_delta;
pub mod version_tuple;
pub mod waiter;
pub mod xl_d_up_tuple;
