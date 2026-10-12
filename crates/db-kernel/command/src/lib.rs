//! Command interpreters for DDL and DML.

#![warn(missing_docs)]

/// Create-filesystem-type command executor.
pub mod create_fs_type;
#[cfg(all(test, not(miri)))]
mod create_fs_type_test;
/// Create-partition-placement command executor.
pub mod create_partition_placement;
#[cfg(test)]
mod create_partition_placement_test;
/// Create-partition-rule command executor.
pub mod create_partition_rule;
#[cfg(test)]
mod create_partition_rule_test;
/// Create-table command executor.
pub mod create_table;
#[cfg(test)]
mod create_table_test;
/// Delete-by-key command executor.
pub mod delete_key_value;
#[cfg(test)]
mod delete_key_value_test;
/// Drop-filesystem-type command executor.
pub mod drop_fs_type;
#[cfg(test)]
mod drop_fs_type_test;
/// Drop-table command executor.
pub mod drop_table;
#[cfg(test)]
mod drop_table_test;
/// DML hooks that create and bind filesystem objects for FS-bound columns.
pub mod fs_hook;
#[cfg(test)]
mod fs_hook_test;
/// Insert command executor.
pub mod insert_key_value;
#[cfg(test)]
mod insert_key_value_test;
/// Load-from-file (COPY FROM) command executor.
pub mod load_from_file;
#[cfg(test)]
mod load_from_file_test;
/// Save-to-file (COPY TO) command executor.
pub mod save_to_file;
#[cfg(test)]
mod save_to_file_test;
/// Update-by-key command executor.
pub mod update_key_value;
#[cfg(test)]
mod update_key_value_test;
