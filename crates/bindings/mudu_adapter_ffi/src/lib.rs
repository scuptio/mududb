//! Versioned native adapter ABI. All allocations are released by this library.
#![warn(missing_docs)]

use mudu::common::result::RS;
use mudu_binding::system::{command_invoke, query_invoke};
use mudu_contract::database::result_batch::ResultBatch;
use std::panic::{AssertUnwindSafe, catch_unwind};
use sys_interface_standalone::sync_api;

/// A library-owned byte buffer. Pass it unchanged to `mudu_adapter_free` once.
#[repr(C)]
pub struct AdapterBuffer {
    /// Address of the first byte, or null for an empty buffer.
    pub data: *mut u8,
    /// Number of bytes at `data`.
    pub len: usize,
}

impl AdapterBuffer {
    fn new(data: Vec<u8>) -> Self {
        let data = data.into_boxed_slice();
        let len = data.len();
        Self {
            data: Box::into_raw(data).cast(),
            len,
        }
    }
}

/// Return the ABI major version; callers must check before calling other exports.
#[unsafe(no_mangle)]
pub extern "C" fn mudu_adapter_abi_version() -> u32 {
    1
}

fn dispatch(operation: &str, input: &[u8]) -> RS<Vec<u8>> {
    match operation {
        "open" => sync_api::mudu_open_bytes(input),
        "close" => sync_api::mudu_close_bytes(input),
        "query" => {
            let (oid, stmt, params) = query_invoke::deserialize_query_param(input)?;
            let result =
                mudu_adapter::syscall::mudu_query::<String>(oid, stmt.as_ref(), params.as_ref())
                    .and_then(|set| {
                        let (rows, desc) = set.into_parts();
                        Ok((
                            ResultBatch::from_result_set(oid, rows.as_ref())?,
                            desc.as_ref().clone(),
                        ))
                    });
            Ok(query_invoke::serialize_query_result(result))
        }
        "command" | "batch" => {
            let (oid, stmt, params) = command_invoke::deserialize_command_param(input)?;
            let result = if operation == "batch" {
                mudu_adapter::syscall::mudu_batch(oid, stmt.as_ref(), params.as_ref())
            } else {
                mudu_adapter::syscall::mudu_command(oid, stmt.as_ref(), params.as_ref())
            };
            Ok(command_invoke::serialize_command_result(result))
        }
        // Queries materialize all rows and return eof=true; no cursor is retained.
        _ => Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "unsupported native adapter operation"
        )),
    }
}

/// Call an operation with an MSSP frame. Zero means success; nonzero means error.
/// On return `output` owns either a response or a UTF-8 diagnostic. Free it once.
///
/// # Safety
/// Input pointers must reference readable arrays of the specified lengths.
/// `output` must be writable and must not alias either input. Null input pointers
/// are allowed only with zero length. Calls must not change process configuration.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mudu_adapter_call(
    operation: *const u8,
    operation_len: usize,
    input: *const u8,
    input_len: usize,
    output: *mut AdapterBuffer,
) -> i32 {
    if output.is_null() {
        return 1;
    }
    unsafe {
        output.write(AdapterBuffer {
            data: std::ptr::null_mut(),
            len: 0,
        });
    }
    if (operation.is_null() && operation_len != 0)
        || (input.is_null() && input_len != 0)
        || operation_len > isize::MAX as usize
        || input_len > isize::MAX as usize
    {
        return 1;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let op = if operation_len == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(operation, operation_len) }
        };
        let input = if input_len == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(input, input_len) }
        };
        let op = std::str::from_utf8(op).map_err(|_| "invalid operation encoding".to_owned())?;
        dispatch(op, input).map_err(|e| format!("adapter operation failed ({:?})", e.ec()))
    }));
    let (status, bytes) = match result {
        Ok(Ok(bytes)) => (0, bytes),
        Ok(Err(message)) => (2, message.into_bytes()),
        Err(_) => (3, b"native adapter panicked".to_vec()),
    };
    unsafe {
        output.write(AdapterBuffer::new(bytes));
    }
    status
}

/// Release a buffer returned by `mudu_adapter_call`.
///
/// # Safety
/// The buffer must be an unchanged, not-yet-freed value returned by this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mudu_adapter_free(buffer: AdapterBuffer) {
    if !buffer.data.is_null() {
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                buffer.data,
                buffer.len,
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_operation_is_owned_error() {
        let mut output = AdapterBuffer {
            data: std::ptr::null_mut(),
            len: 0,
        };
        let status =
            unsafe { mudu_adapter_call(b"unknown".as_ptr(), 7, std::ptr::null(), 0, &mut output) };
        assert_eq!(status, 2);
        assert!(output.len > 0);
        unsafe {
            mudu_adapter_free(output);
        }
    }
    #[test]
    fn rejects_null_input_before_dereferencing() {
        let mut output = AdapterBuffer {
            data: std::ptr::null_mut(),
            len: 0,
        };
        assert_eq!(
            unsafe { mudu_adapter_call(std::ptr::null(), 1, std::ptr::null(), 0, &mut output) },
            1
        );
        assert!(output.data.is_null());
    }
}
