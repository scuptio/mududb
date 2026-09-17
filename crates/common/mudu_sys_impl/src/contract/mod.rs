//! Contracts (traits) that system implementations must satisfy.
#![allow(missing_docs)]
// Pure trait/definition modules live in mudu_sys_contract and are re-exported here.
pub use mudu_sys_contract::contract::{
    async_file, async_fs, async_io_provider, async_listener, async_mode, async_net, async_stream,
    file_options, io_provider_base, task_async,
};

// These modules depend on the implementation (Sys, io, net, etc.)
// and therefore remain in mudu_sys_impl.
pub mod sync;
