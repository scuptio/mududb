use crate::error::ApiError;

#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
mod generated {
    wit_bindgen::generate!({
        path: "wit",
        world: "async-api",
        async: true,
    });
}

/// Forwards one MSSP frame to the `system::*` import of the same name.
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
macro_rules! forward_raw {
    ($name:ident, $import:ident) => {
        pub async fn $name(frame: Vec<u8>) -> Result<Vec<u8>, ApiError> {
            Ok(generated::mududb::async_api::system::$import(frame).await)
        }
    };
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(query_raw, query);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(command_raw, command);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(batch_raw, batch);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(open_raw, open);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(close_raw, close);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(get_raw, get);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(put_raw, put);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(delete_raw, delete);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(range_raw, range);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(relation_get_raw, relation_get);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(relation_update_raw, relation_update);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(relation_insert_raw, relation_insert);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(fs_open_raw, fs_open);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(fs_close_raw, fs_close);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(fs_read_raw, fs_read);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(fs_write_raw, fs_write);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(fs_pread_raw, fs_pread);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(fs_pwrite_raw, fs_pwrite);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(fs_lseek_raw, fs_lseek);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(fs_fstat_raw, fs_fstat);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(fs_stat_raw, fs_stat);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(fs_fsync_raw, fs_fsync);
#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
forward_raw!(fs_readdir_raw, fs_readdir);

#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
pub async fn fetch_raw(query_result: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    Ok(generated::mududb::async_api::system::fetch(query_result).await)
}
