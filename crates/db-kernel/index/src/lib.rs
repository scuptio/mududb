//! B-tree index implementation and key encodings used by the execution layer.

#![warn(missing_docs)]

/// Ordered map index over encoded key tuples.
pub mod btree;
/// Index key encoding and the thread-local comparator context.
pub mod index_key;
