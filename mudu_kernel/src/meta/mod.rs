//! In-memory catalog managers for schemas, partitions, rules, and placements.

#![allow(missing_docs)]

pub mod _fuzz;

pub mod meta_mgr;
pub mod meta_mgr_factory;
pub mod partition_binding_catalog;
pub mod partition_placement_catalog;
pub mod partition_rule_catalog;
pub mod schema_catalog;
