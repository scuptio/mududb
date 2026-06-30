//! Native x86_64 YCSB stored-procedure implementations.

/// Synchronous YCSB procedures.
#[allow(missing_docs)]
pub mod procedure;

/// Asynchronous YCSB procedures.
#[allow(missing_docs)]
pub mod procedure_async;

/// Shared helpers for YCSB procedures.
#[allow(missing_docs)]
pub(crate) mod procedure_common;

#[cfg(test)]
mod procedure_test;

#[cfg(test)]
mod procedure_async_test;

#[cfg(test)]
mod procedure_common_test;
