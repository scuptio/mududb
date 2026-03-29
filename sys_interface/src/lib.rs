pub mod api;
mod api_impl;
pub mod async_api;
pub mod host;
pub mod sync_api;
#[cfg(feature = "uniffi-bindings")]
pub mod uniffi;

#[cfg(feature = "uniffi-bindings")]
::uniffi::setup_scaffolding!();

#[cfg(all(
    target_arch = "wasm32",
    feature = "wasip1",
    not(feature = "component-model")
))]
pub mod extern_c;
#[cfg(all(
    target_arch = "wasm32",
    feature = "component-model",
    not(feature = "async")
))]
mod inner_component;
#[cfg(all(target_arch = "wasm32", feature = "component-model", feature = "async"))]
mod inner_component_async;
#[cfg(all(
    target_arch = "wasm32",
    feature = "wasip1",
    not(feature = "component-model")
))]
pub mod inner_p1;
