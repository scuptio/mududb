//! Optional native adapter transport. Configure MUDU_CONNECTION before selecting it.
use crate::error::ApiError;
use libloading::Library;
use std::{path::Path, sync::OnceLock};

#[repr(C)]
struct Buffer {
    data: *mut u8,
    len: usize,
}
type Call = unsafe extern "C" fn(*const u8, usize, *const u8, usize, *mut Buffer) -> i32;
type Free = unsafe extern "C" fn(Buffer);
struct Adapter {
    _library: Library,
    call: Call,
    free: Free,
}
static ADAPTER: OnceLock<Adapter> = OnceLock::new();

/// Select an ABI v1 runtime for process lifetime. Only load a trusted library.
/// The runtime must match this process's OS and architecture.
pub fn use_adapter(path: impl AsRef<Path>) -> Result<(), ApiError> {
    // SAFETY: the user explicitly selects a native library implementing ABI v1.
    let library = unsafe { Library::new(path.as_ref()) }.map_err(|_| {
        ApiError::backend_unavailable(
            "cannot load adapter: check path, architecture and native dependencies",
        )
    })?;
    unsafe {
        let version = library
            .get::<unsafe extern "C" fn() -> u32>(b"mudu_adapter_abi_version\0")
            .map_err(|_| ApiError::backend_unavailable("missing adapter ABI version"))?;
        if version() != 1 {
            return Err(ApiError::backend_unavailable("adapter ABI v1 required"));
        }
        let call = *library
            .get::<Call>(b"mudu_adapter_call\0")
            .map_err(|_| ApiError::backend_unavailable("missing adapter call export"))?;
        let free = *library
            .get::<Free>(b"mudu_adapter_free\0")
            .map_err(|_| ApiError::backend_unavailable("missing adapter free export"))?;
        ADAPTER
            .set(Adapter {
                _library: library,
                call,
                free,
            })
            .map_err(|_| ApiError::backend_unavailable("adapter already selected"))
    }
}

pub(crate) fn enabled() -> bool {
    ADAPTER.get().is_some()
}

/// Send an existing MSSP frame; copies and releases the native result.
pub fn call(operation: &str, frame: &[u8]) -> Result<Vec<u8>, ApiError> {
    let adapter = ADAPTER
        .get()
        .ok_or_else(|| ApiError::backend_unavailable("select adapter first"))?;
    let mut output = Buffer {
        data: std::ptr::null_mut(),
        len: 0,
    };
    // SAFETY: input slices live through the call and the checked ABI owns output.
    let status = unsafe {
        (adapter.call)(
            operation.as_ptr(),
            operation.len(),
            frame.as_ptr(),
            frame.len(),
            &mut output,
        )
    };
    let bytes = if output.len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(output.data, output.len).to_vec() }
    };
    unsafe {
        (adapter.free)(output);
    }
    if status != 0 {
        return Err(ApiError::Decode(
            String::from_utf8_lossy(&bytes).into_owned(),
        ));
    }
    Ok(bytes)
}
