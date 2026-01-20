pub mod api;
mod host;

#[cfg(all(target_arch = "wasm32", feature = "wasip1", not(feature = "wasip2")))]
pub mod inner_p1;
#[cfg(all(target_arch = "wasm32", feature = "wasip1", not(feature = "wasip2")))]
pub mod extern_c;
#[cfg(all(target_arch = "wasm32", feature = "wasip2", not(feature = "async")))]
mod inner_p2;
#[cfg(all(target_arch = "wasm32", feature = "wasip2", feature = "async"))]
mod inner_p2_async;


