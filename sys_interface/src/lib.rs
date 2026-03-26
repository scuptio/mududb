pub mod api;
pub mod host;

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
