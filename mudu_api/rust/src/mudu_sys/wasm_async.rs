use crate::error::ApiError;

#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
mod generated {
    wit_bindgen::generate!({
        path: "wit",
        world: "async-api",
        async: true,
    });
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
pub async fn query_raw(query_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    Ok(generated::mududb::async_api::system::query(&query_in).await)
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
pub async fn command_raw(command_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    Ok(generated::mududb::async_api::system::command(&command_in).await)
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
pub async fn fetch_raw(query_result: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    Ok(generated::mududb::async_api::system::fetch(&query_result).await)
}
