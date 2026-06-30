//! Async IO provider traits and dispatch.
#![allow(missing_docs)]
use crate::common::provider_type::ProviderType;
use crate::contract::async_io_provider::AsyncIoProvider;
use crate::imp;
use std::sync::Arc;

pub fn create_io_provider(mode: ProviderType) -> Arc<dyn AsyncIoProvider> {
    imp::runtime::create_async_runtime(mode)
}
