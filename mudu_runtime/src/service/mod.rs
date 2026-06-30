//! Core runtime services for loading, installing and invoking Mudu packages.

#![allow(clippy::module_inception)]
/// Application instance trait.
pub mod app_inst;
/// Application instance implementation.
pub mod app_inst_impl;
mod file_name;
pub(crate) mod mudu_package;
#[cfg(test)]
mod mudu_package_test;
/// WebAssembly package module wrapper.
pub mod package_module;
/// Runtime trait definitions.
pub mod runtime;
/// Runtime implementation helpers.
pub mod runtime_impl;
#[cfg(test)]
mod runtime_impl_test;
mod runtime_simple;
#[cfg(test)]
mod test_wasm_mod_path;

/// Component responsible for invoking procedures.
pub mod procedure_invoke_component;
#[cfg(test)]
mod runtime_simple_test;
/// Service task registry and execution.
pub mod service;
mod service_impl;
mod service_trait;
/// Pre-instantiated Wasmtime component wrapper.
pub mod wt_instance_pre;

mod wt_runtime;

mod kernel_function_p2;
#[cfg(test)]
mod kernel_function_p2_test;
/// Runtime option structures.
pub mod runtime_opt;
mod wasi_context_component;
#[cfg(test)]
mod wasi_context_component_test;
mod wt_runtime_component;

/// Application list types.
pub mod app_list;
mod kernel_function_p2_async;
#[cfg(test)]
mod kernel_function_p2_async_test;
#[cfg(test)]
mod wt_runtime_component_test;
