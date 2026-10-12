//! Procedure descriptors, invocation parameters, and result wrappers.

/// Procedure descriptions grouped by module.
pub mod mod_proc_desc;
#[cfg(test)]
mod mod_proc_desc_test;
/// Name prefixes used when exporting procedures from guest modules.
pub mod proc;
/// Procedure signature description (parameters and return types).
pub mod proc_desc;
#[cfg(test)]
mod proc_desc_test;
/// Parameters of a procedure invocation.
pub mod procedure_param;
#[cfg(test)]
mod procedure_param_test;
/// Result values of a procedure invocation.
pub mod procedure_result;
#[cfg(test)]
mod procedure_result_test;
