//! Relation storage: an MVCC row layer over two time-series files (keys and
//! values) with an in-memory index.

#![allow(clippy::module_inception)]
pub mod relation;
