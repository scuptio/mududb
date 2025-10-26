use crate::procedure::wasi_context::{ContextData, WasiContext};
use mudu::common::endian::write_u32;
use mudu::common::result::RS;
use mudu::common::result_of::rs_option;
use mudu::common::serde_utils::{deserialize_sized_from, header_size_len, serialize_sized_to, serialize_sized_to_vec};
use mudu::common::xid::XID;
use mudu::database::err_no;
use mudu::database::sql::Context;
use mudu::database::sql_params::SQLParams;
use mudu::database::v2h_param::{
    CommandIn, CommandOut, QueryIn,
    QueryResult, ResultCursor, ResultRow,
};
use mudu::error::ec::EC;
use mudu::error::err::MError;
use mudu::m_error;
use mudu::tuple::datum::DatumDyn;
use mudu::tuple::typed_bin::TypedBin;
use serde::de::DeserializeOwned;
use serde::Serialize;
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
    handle_guest_invoke_host::<QueryIn, QueryResult, _>(
        caller,
        param_buf_ptr,
        param_buf_len,
        out_buf_ptr,
        out_buf_len,
        out_mem_ptr,
        out_mem_len,
        query_gut,
    )
}

/// Fetch next row from a query result cursor
pub fn kernel_fetch(
    caller: Caller<'_, WasiContext>,
    param_buf_ptr: u32, param_buf_len: u32,
    output_buf_ptr: u32, output_buf_len: u32,
    out_mem_ptr: u32,
    out_mem_len: u32,
) -> i32 {
    handle_guest_invoke_host::<ResultCursor, ResultRow, _>(
        caller,
        param_buf_ptr,
        param_buf_len,
        output_buf_ptr,
        output_buf_len,
        out_mem_ptr,
        out_mem_len,
        query_fetch_gut,
    )
}

/// Execute a SQL command (INSERT, UPDATE, DELETE) from WebAssembly guest
pub fn kernel_command(
    caller: Caller<'_, WasiContext>,
    param_buf_ptr: u32, param_buf_len: u32,
    output_buf_ptr: u32, output_buf_len: u32,
    out_mem_ptr: u32,
    out_mem_len: u32,
) -> i32 {
    handle_guest_invoke_host::<CommandIn, CommandOut, _>(
        caller,
        param_buf_ptr,
        param_buf_len,
        output_buf_ptr,
        output_buf_len,
        out_mem_ptr,
        out_mem_len,
        command_gut,
    )
}

/// Retrieve memory chunk by ID from the context
pub fn kernel_get_memory(
    caller: Caller<'_, WasiContext>,
    mem_id: u32,
    output_buf_ptr: u32,
    output_buf_len: u32,
) -> i32 {
    let opt_mem = {
        caller.data().context_ref().get_memory(mem_id)
    };
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
        None => { -1 }
    }
}

// =============================================================================
// Core Business Logic
// =============================================================================

/// Execute a SQL query with parameters
fn query_gut(
    query_in: &QueryIn,
) -> RS<QueryResult> {
    let xid = query_in.xid();
    let context = get_context(xid)?;
    let params: Vec<Box<dyn DatumDyn>> = query_in
        .param()
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let dat_type_id = query_in.param_desc()[i].dat_type_id();
            Box::new(TypedBin::new(dat_type_id, e.clone())) as Box<dyn DatumDyn>
        })
        .collect();
    let result = context.query_raw(
        &query_in.sql(),
        &params as &dyn SQLParams,
    )?;
    let rs = context.cache_result(result)?;
    Ok(rs)
}

/// Fetch the next row from a result cursor
fn query_fetch_gut(query_cursor: &ResultCursor) -> RS<ResultRow> {
    let context = get_context(query_cursor.xid())?;
    let opt_tuple = context.query_next()?;
    Ok(ResultRow::new(opt_tuple))
}

/// Execute a SQL command with parameters
fn command_gut(
    command_in: &CommandIn
) -> RS<CommandOut> {
    let xid = command_in.xid();
    let context = get_context(xid)?;
    let param: Vec<Box<dyn DatumDyn>> = command_in
        .param()
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let dat_type_id = command_in.param_desc().fields()[i].dat_type_id();
            Box::new(TypedBin::new(dat_type_id, e.clone())) as Box<dyn DatumDyn>
        })
        .collect();
    let affected_rows = context.command(
        &command_in.sql(),
        &param as &dyn SQLParams,
    )?;
    Ok(CommandOut::new(affected_rows))
}

fn get_context(xid: XID) -> RS<Context> {
    let opt = Context::context(xid);
    let context = rs_option(opt, &format!("no such transaction {}", xid))?;
    Ok(context)
}


fn deserialize_input<
    D: DeserializeOwned + 'static,
>(
    caller: &Caller<'_, WasiContext>,
    memory: &Memory,
    param_buf_ptr: u32, param_buf_len: u32,
) -> RS<D> {
    let buf = memory.data(&caller);
    let _buf_ptr = param_buf_ptr as usize;
    let _buf_len = param_buf_len as usize;
    check_bounds(_buf_ptr, _buf_len, buf.len())?;
    let in_param = &buf[_buf_ptr.._buf_ptr + _buf_len];

    let (d, _size): (D, u64) = deserialize_sized_from(in_param)?;
    Ok(d)
}

fn serialize_and_write_output<
    S: Serialize + 'static,
>(
    result: S,
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
    let out_param = &mut mem_buf[_out_buf_ptr.._out_buf_ptr + _out_buf_len];
    let (ok, size) = serialize_sized_to(&result, out_param)?;
    let _out_len_ptr = out_len_ptr as usize;
    let total_size = size as u32 + header_size_len() as u32;
    let _mem_id_ptr = mem_id_ptr as usize;

    // write the expected output buffer size
    check_bounds(_out_len_ptr, size_of::<u32>(), mem_buf.len())?;
    write_u32(&mut mem_buf[_out_len_ptr.._out_len_ptr + size_of::<u32>()], total_size);
    if !ok {
        handle_insufficient_buffer(result, context, mem_buf, mem_id_ptr)
    } else {
        Ok(err_no::EN_OK)
    }
}


/// Handle case when output buffer is too small
fn handle_insufficient_buffer<Output: Serialize>(
    result: Output,
    context_ptr: *const ContextData,
    memory_data: &mut [u8],
    mem_id_ptr: u32,
) -> RS<i32> {
    let serialized_data = serialize_sized_to_vec(&result)?;
    let context_ref = unsafe { &*context_ptr };

    let memory_id = context_ref.add_memory(serialized_data);
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
    D: DeserializeOwned + 'static,
    S: Serialize + 'static,
    F: Fn(&D) -> RS<S> + 'static,
>(
    caller: &mut Caller<'_, WasiContext>,
    context: *const ContextData,
    memory: &Memory,
    param_buf_ptr: u32, param_buf_len: u32,
    output_buf_ptr: u32, output_buf_len: u32,
    out_len_ptr: u32,
    mem_id_ptr: u32,
    f: F,
) -> (i32, Option<MError>) {
    // Deserialize input parameters
    let input = match deserialize_input::<D>(caller, memory, param_buf_ptr, param_buf_len) {
        Ok(input) => {
            input
        }
        Err(e) => {
            return (err_no::EN_DECODE_PARAM, Some(e));
        }
    };

    // Execute the handler function
    let result = match f(&input) {
        Ok(s) => s,
        Err(_e) => {
            return (err_no::EN_INVOKE, Some(_e));
        }
    };

    // Serialize and write output
    match serialize_and_write_output(
        result, context,
        caller,
        &memory,
        output_buf_ptr,
        output_buf_len,
        out_len_ptr,
        mem_id_ptr,
    ) {
        Ok(output) => {
            (output, None)
        }
        Err(e) => {
            (err_no::EN_ENCODE_RESULT, Some(e))
        }
    }
}

/// Generic handler for WebAssembly guest to host invocations
fn handle_guest_invoke_host<
    D: DeserializeOwned + 'static,
    S: Serialize + 'static,
    F: Fn(&D) -> RS<S> + 'static,
>(
    caller: Caller<'_, WasiContext>,
    param_buf_ptr: u32, param_buf_len: u32,
    output_buf_ptr: u32, output_buf_len: u32,
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
    let (result_code, opt_error) = handle_wasm_guest_invoke_host_gut(
        &mut caller,
        context,
        &memory,
        param_buf_ptr, param_buf_len,
        output_buf_ptr, output_buf_len,
        out_len_ptr,
        mem_id_ptr,
        function,
    );
    // If there was an error, write it to the output buffer
    if let Some(e) = opt_error {
        let _ = write_error_response(
            &mut caller,
            context,
            &memory,
            output_buf_ptr,
            output_buf_len,
            out_len_ptr,
            mem_id_ptr,
            e,
        );
    }
    result_code
}

/// Write error response to guest memory
fn write_error_response(
    caller: &mut Caller<'_, WasiContext>,
    context_ptr: *const ContextData,
    memory: &Memory,
    output_buf_ptr: u32,
    output_buf_len: u32,
    out_len_ptr: u32,
    mem_id_ptr: u32,
    error: MError,
) -> RS<()> {
    let _ = serialize_and_write_output(
        error,
        context_ptr,
        caller,
        memory,
        output_buf_ptr,
        output_buf_len,
        out_len_ptr,
        mem_id_ptr,
    )?;
    Ok(())
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