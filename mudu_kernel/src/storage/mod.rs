//! On-disk storage layer: pages, relations, and time-series files.
//!
//! This module contains the page format, record slot layout, relation
//! abstractions, and time-series file implementations used by the engine.

#![allow(missing_docs)]

pub mod page;
pub mod relation;
pub mod time_series;
