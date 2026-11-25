//! The kind of a class (a DDL-declared storage/transport carrier).

use serde::{Deserialize, Serialize};

/// The carrier an object class declared in DDL maps to.
///
/// This is the definition-side `kind` tag shared by the binding and catalog
/// layers: the same named class (fields/types/nullability/keys) is tagged with
/// the carrier it belongs to. `Type` is a transient shape-only declaration
/// (`CREATE TYPE ... AS (...)`); `Cache` mirrors an existing table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClassKind {
    /// A durable table (`CREATE TABLE`), the default.
    #[default]
    Table,
    /// An in-database message queue (`CREATE QUEUE`).
    Queue,
    /// A volatile, non-replicated in-memory cache mirroring a table
    /// (`CREATE CACHE ... LIKE <table>`).
    Cache,
    /// A transient type-only declaration (`CREATE TYPE ... AS (...)`).
    Type,
}

impl ClassKind {
    /// Returns `true` for the durable table carrier.
    pub fn is_table(&self) -> bool {
        matches!(self, Self::Table)
    }

    /// Returns `true` for the message-queue carrier.
    pub fn is_queue(&self) -> bool {
        matches!(self, Self::Queue)
    }

    /// Returns `true` for the volatile cache carrier.
    pub fn is_cache(&self) -> bool {
        matches!(self, Self::Cache)
    }

    /// Returns `true` for the transient type-only declaration.
    pub fn is_type(&self) -> bool {
        matches!(self, Self::Type)
    }
}
