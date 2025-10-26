use crate::sys_call;
use mudu::common::result::RS;
use mudu::common::serde_utils::{deserialize_sized_from, serialize_sized_to_vec};
use mudu::common::xid::XID;
use mudu::database::record::Record;
use mudu::database::record_set::RecordSet;
use mudu::database::result_set::ResultSet;
use mudu::database::sql_stmt::SQLStmt;
use mudu::database::v2h_param::{CommandIn, CommandOut, QueryIn, QueryResult, ResultCursor, ResultRow};
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::tuple::tuple_field::TupleField;
use serde::Serialize;
use std::sync::Arc;
use mudu::common::endian::read_u32;
use mudu::database::sql_params::SQLParams;
use mudu::error::err::MError;

pub fn inner_query<R: Record>(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    let tuple_desc = params.tuple_desc()?;
    let vec_bin = params.to_binary(tuple_desc.fields())?;
    let str_sql = sql.to_string();
    let query_in = QueryIn::new(
        xid,
        str_sql,
        vec_bin,
        tuple_desc,
    );
    let result = _sys_query(&query_in)?;
    let cursor = result.cursor();
    let record_set = RecordSet::<R>::new(
        Arc::new(ResultSetWrapper::new(cursor)), Arc::new(result.into_tuple_desc()));
    Ok(record_set)
}

pub fn inner_command(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    let tuple_desc = params.tuple_desc()?;
    let vec_bin = params.to_binary(tuple_desc.fields())?;
    let str_sql = sql.to_string();
    let command_in = CommandIn::new(
        xid,
        str_sql,
        vec_bin,
        tuple_desc,
    );
    let result = _sys_command(&command_in)?;
    Ok(result.affected_rows())
}

pub struct ResultSetWrapper {
    cursor: ResultCursor,
}

impl ResultSetWrapper {
    pub fn new(cursor: ResultCursor) -> ResultSetWrapper {
        ResultSetWrapper { cursor }
    }
}

impl ResultSet for ResultSetWrapper {
    fn next(&self) -> RS<Option<TupleField>> {
        let result_row = _sys_fetch(&self.cursor)?;
        Ok(result_row.into_result())
    }
}

fn _sys_query(query_in: &QueryIn) -> RS<QueryResult> {
    let out_mem = __sys_query(query_in)?;
    let (query_result, _): (QueryResult, _) = deserialize_sized_from(out_mem.slice())?;
    Ok(query_result)
}

fn _sys_command(command_in: &CommandIn) -> RS<CommandOut> {
    let out_mem = __sys_command(command_in)?;
    let (command_out, _): (CommandOut, _) = deserialize_sized_from(out_mem.slice())?;
    Ok(command_out)
}

fn _sys_fetch(cursor: &ResultCursor) -> RS<ResultRow> {
    let out_mem = __sys_fetch(cursor)?;
    let (result_row, _): (ResultRow, _) = deserialize_sized_from(out_mem.slice())?;
    Ok(result_row)
}

struct OutMemory {
    pub len: u32,
    pub vec: Vec<u8>,
}

impl Default for OutMemory {
    fn default() -> OutMemory {
        Self {
            vec: vec![0; 512],
            len: 0,
        }
    }
}


impl OutMemory {
    fn slice(&self) -> &[u8] {
        &self.vec
    }

    fn slice_mut(&mut self) -> &mut [u8] {
        &mut self.vec
    }
}

fn __sys_call<
    P: Serialize,
    F: Fn(
        *const u8, usize,
        *mut u8, usize,
        *mut u8,
        *mut u8,
    ) -> i32
>(
    param: &P,
    sys_fn: F,
    fn_name: &'static str,
) -> RS<OutMemory> {
    let param = serialize_sized_to_vec(param)?;
    let mut out_mem = OutMemory::default();
    let ret_value = {
        let mut out_mem_len = [0u8;size_of::<u32>()];
        let mut out_mem_id = [0u8;size_of::<u32>()];
        let n = sys_fn(
            param.as_ptr(), param.len(),
            out_mem.slice_mut().as_mut_ptr(),
            out_mem.slice().len(),
            out_mem_len.as_mut_ptr(),
            out_mem_id.as_mut_ptr(),
        );
        let mem_id = read_u32(&out_mem_id);
        out_mem.len = read_u32(&out_mem_len);
        if mem_id != 0 { // the provided memory is insufficient
            out_mem.vec.resize(out_mem.len as usize, 0);
            let size = unsafe {
                sys_call::sys_get_memory(
                    mem_id,
                    out_mem.slice_mut().as_mut_ptr(),
                    out_mem.slice().len(), )
            };
            if size != out_mem.len as i32 {
                panic!("output memory does not match expected size")
            }
        }
        n
    };
    if ret_value != 0 {
        let (mut err, _len) = deserialize_sized_from::<MError>(&out_mem.slice()[0..out_mem.len as usize])
            .unwrap_or((m_error!(EC::MuduError, "cannot deserialized error"), 0));
        out_mem.vec.resize(0, 0);
        out_mem.len = 0;
        err.set_message(format!("sys call {} error, return code:{}, {}", fn_name, ret_value, err.message()));
        return Err(err);
    }
    Ok(out_mem)
}

fn __sys_query(query_in: &QueryIn) -> RS<OutMemory> {
    __sys_call(query_in, ___sys_query, "sys_query")
}

fn __sys_command(command_in: &CommandIn) -> RS<OutMemory> {
    __sys_call(command_in, ___sys_command, "sys_command")
}

fn __sys_fetch(result_cursor: &ResultCursor) -> RS<OutMemory> {
    __sys_call(result_cursor, ___sys_fetch, "sys_fetch")
}


fn ___sys_query(
    param_buf_ptr: *const u8,
    param_buf_len: usize,
    out_buf_ptr: *mut u8,
    out_buf_len: usize,
    out_len: *mut u8,
    mem_id: *mut u8,
) -> i32 {
    unsafe {
        sys_call::sys_query(
            param_buf_ptr,
            param_buf_len,
            out_buf_ptr,
            out_buf_len,
            out_len,
            mem_id,
        )
    }
}


fn ___sys_command(
    param_buf_ptr: *const u8,
    param_buf_len: usize,
    out_buf_ptr: *mut u8,
    out_buf_len: usize,
    out_len: *mut u8,
    mem_id: *mut u8,
) -> i32 {
    unsafe {
        sys_call::sys_command(
            param_buf_ptr,
            param_buf_len,
            out_buf_ptr,
            out_buf_len,
            out_len,
            mem_id,
        )
    }
}


fn ___sys_fetch(
    param_buf_ptr: *const u8,
    param_buf_len: usize,
    out_buf_ptr: *mut u8,
    out_buf_len: usize,
    out_len: *mut u8,
    mem_id: *mut u8,
) -> i32 {
    unsafe {
        sys_call::sys_fetch(
            param_buf_ptr,
            param_buf_len,
            out_buf_ptr,
            out_buf_len,
            out_len,
            mem_id,
        )
    }
}