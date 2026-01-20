use crate::interface::kernel;
use crate::procedure::wasi_context::{ContextData, WasiContext};
use mudu::common::endian::write_u32;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::error::err::MError;
use mudu::m_error;
use mudu_contract::database::err_no;
use std::cmp::min;
use wasmtime::{Caller, Extern, Memory};

// =============================================================================
// Public Kernel Interface Functions
// =============================================================================
/// Execute a SQL query from WebAssembly guest
pub fn kernel_query(
    caller: Caller<'_, WasiContext>,
    param_buf_ptr: u32,
    param_buf_len: u32,
    out_buf_ptr: u32,
    out_buf_len: u32,
    out_mem_ptr: u32,
    out_mem_len: u32,
) -> i32 {
    handle_guest_invoke_host::<_>(
        caller,
        param_buf_ptr,
        param_buf_len,
        out_buf_ptr,
        out_buf_len,
        out_mem_ptr,
        out_mem_len,
        kernel::query_internal,
    )
}

/// Fetch next row from a query result cursor
pub fn kernel_fetch_p1(
    caller: Caller<'_, WasiContext>,
    param_buf_ptr: u32,
    param_buf_len: u32,
    output_buf_ptr: u32,
    output_buf_len: u32,
    out_mem_ptr: u32,
    out_mem_len: u32,
) -> i32 {
    handle_guest_invoke_host::<_>(
        caller,
        param_buf_ptr,
        param_buf_len,
        output_buf_ptr,
        output_buf_len,
        out_mem_ptr,
        out_mem_len,
        kernel::fetch_internal,
    )
}

/// Execute a SQL command (INSERT, UPDATE, DELETE) from WebAssembly guest
pub fn kernel_command_p1(
    caller: Caller<'_, WasiContext>,
    param_buf_ptr: u32,
    param_buf_len: u32,
    output_buf_ptr: u32,
    output_buf_len: u32,
    out_mem_ptr: u32,
    out_mem_len: u32,
) -> i32 {
    handle_guest_invoke_host::<_>(
        caller,
        param_buf_ptr,
        param_buf_len,
        output_buf_ptr,
        output_buf_len,
        out_mem_ptr,
        out_mem_len,
        kernel::command_internal,
    )
}

/// Retrieve memory chunk by ID from the context
pub fn kernel_get_memory_p1(
    caller: Caller<'_, WasiContext>,
    mem_id: u32,
    output_buf_ptr: u32,
    output_buf_len: u32,
) -> i32 {
    let opt_mem = { caller.data().context_ref().get_memory(mem_id) };
    match opt_mem {
        Some(mem) => {
            let size = min(mem.len(), output_buf_len as usize);
            let mut caller = caller;
            let memory = get_memory(&mut caller).unwrap();
            let data = memory.data_mut(&mut caller);
            let _output_buf_ptr = output_buf_ptr as usize;
            let _output_buf_len = output_buf_len as usize;
            check_bounds(_output_buf_ptr, _output_buf_len, data.len()).unwrap();
            data[_output_buf_ptr.._output_buf_ptr + size].copy_from_slice(&mem[..size]);
            size as i32
        }
        None => -1,
    }
}

// =============================================================================
// Kernel Function
// =============================================================================


fn serialize_and_write_output(
    result: Vec<u8>,
    context: *const ContextData,
    caller: &mut Caller<'_, WasiContext>,
    memory: &Memory,
    output_buf_ptr: u32,
    output_buf_len: u32,
    out_len_ptr: u32,
    mem_id_ptr: u32,
) -> RS<i32> {
    let mem_buf = memory.data_mut(caller);
    let _out_buf_ptr = output_buf_ptr as usize;
    let _out_buf_len = output_buf_len as usize;
    check_bounds(_out_buf_ptr, _out_buf_len, mem_buf.len())?;
    let _out_len_ptr = out_len_ptr as usize;
    let total_size = result.len() as u32;
    let _mem_id_ptr = mem_id_ptr as usize;

    // write the expected output buffer size
    check_bounds(_out_len_ptr, size_of::<u32>(), mem_buf.len())?;
    write_u32(
        &mut mem_buf[_out_len_ptr.._out_len_ptr + size_of::<u32>()],
        total_size,
    );
    let out_param = &mut mem_buf[_out_buf_ptr.._out_buf_ptr + _out_buf_len];
    if result.len() > out_param.len() {
        handle_insufficient_buffer(result, context, mem_buf, mem_id_ptr)
    } else {
        out_param.copy_from_slice(&result);
        Ok(err_no::EN_OK)
    }
}

/// Handle case when output buffer is too small
fn handle_insufficient_buffer(
    result: Vec<u8>,
    context_ptr: *const ContextData,
    memory_data: &mut [u8],
    mem_id_ptr: u32,
) -> RS<i32> {
    let context_ref = unsafe { &*context_ptr };
    let memory_id = context_ref.add_memory(result);
    write_memory_id(memory_data, mem_id_ptr, memory_id)?;
    Ok(err_no::EN_INSUFFICIENT_BUFFER_LENGTH_FOR_OUTPUT)
}

fn write_memory_id(memory_data: &mut [u8], mem_id_ptr: u32, memory_id: u32) -> RS<()> {
    let ptr = mem_id_ptr as usize;
    check_bounds(ptr, size_of::<u32>(), memory_data.len())?;
    write_u32(&mut memory_data[ptr..ptr + size_of::<u32>()], memory_id);
    Ok(())
}
// =============================================================================
// Core Host-Guest Communication
// =============================================================================
/// Process a single host invocation with error handling
fn handle_wasm_guest_invoke_host_gut<
    F: Fn(&[u8]) -> Vec<u8> + 'static,
>(
    caller: &mut Caller<'_, WasiContext>,
    context: *const ContextData,
    memory: &Memory,
    param_buf_ptr: u32,
    param_buf_len: u32,
    output_buf_ptr: u32,
    output_buf_len: u32,
    out_len_ptr: u32,
    mem_id_ptr: u32,
    f: F,
) -> (i32, Option<MError>) {
    let buf= memory.data(&caller);
    let _buf_ptr = param_buf_ptr as usize;
    let _buf_len = param_buf_len as usize;
    let r = check_bounds(_buf_ptr, _buf_len, buf.len());
    match r {
        Err(e) => {
            return (err_no::EN_DECODE_PARAM, Some(e));
        }
        _ => {}
    }
    let in_param = &buf[_buf_ptr.._buf_ptr + _buf_len];

    // Execute the handler function
    let result = f(in_param);

    // Serialize and write output
    match serialize_and_write_output(
        result,
        context,
        caller,
        &memory,
        output_buf_ptr,
        output_buf_len,
        out_len_ptr,
        mem_id_ptr,
    ) {
        Ok(output) => (output, None),
        Err(e) => (err_no::EN_ENCODE_RESULT, Some(e)),
    }
}

/// Generic handler for WebAssembly guest to host invocations
fn handle_guest_invoke_host<
    F: Fn(&[u8]) -> Vec<u8> + 'static,
>(
    caller: Caller<'_, WasiContext>,
    param_buf_ptr: u32,
    param_buf_len: u32,
    output_buf_ptr: u32,
    output_buf_len: u32,
    out_len_ptr: u32,
    mem_id_ptr: u32,
    function: F,
) -> i32 {
    let context = caller.data().context_ptr();
    let mut caller = caller;

    let memory = match get_memory(&mut caller) {
        Ok(mem) => mem,
        Err(_) => return err_no::EN_NO_OUTPUT_MEMORY,
    };
    let (result_code, _) = handle_wasm_guest_invoke_host_gut(
        &mut caller,
        context,
        &memory,
        param_buf_ptr,
        param_buf_len,
        output_buf_ptr,
        output_buf_len,
        out_len_ptr,
        mem_id_ptr,
        function,
    );
    result_code
}

/// Validate memory access bounds
fn check_bounds(ptr: usize, len: usize, memory_size: usize) -> RS<()> {
    if ptr + len > memory_size {
        Err(m_error!(EC::WASMMemoryAccessError, "memory bound error"))
    } else {
        Ok(())
    }
}

fn get_memory(caller: &mut Caller<'_, WasiContext>) -> RS<Memory> {
    match caller.get_export("memory") {
        Some(Extern::Memory(mem)) => Ok(mem),
        _ => Err(m_error!(EC::MuduError, "get memory export error")),
    }
}
