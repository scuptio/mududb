use crate::extern_c;
use crate::host::{
    invoke_host_close, invoke_host_command, invoke_host_open, invoke_host_query,
    invoke_host_session_get, invoke_host_session_put, invoke_host_session_range,
};
use mudu::common::endian::read_u32;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

#[allow(unused)]
pub fn inner_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    invoke_host_query(oid, sql, params, __sys_query)
}

#[allow(unused)]
pub fn inner_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    invoke_host_command(oid, sql, params, __sys_command)
}

#[allow(unused)]
pub fn inner_open() -> RS<OID> {
    invoke_host_open(__sys_open)
}

#[allow(unused)]
pub fn inner_close(session_id: OID) -> RS<()> {
    invoke_host_close(session_id, __sys_close)
}

#[allow(unused)]
pub fn inner_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    invoke_host_session_get(session_id, key, __sys_get)
}

#[allow(unused)]
pub fn inner_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    invoke_host_session_put(session_id, key, value, __sys_put)
}

#[allow(unused)]
pub fn inner_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    invoke_host_session_range(session_id, start_key, end_key, __sys_range)
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
    fn into(self) -> Vec<u8> {
        self.vec
    }
    fn slice(&self) -> &[u8] {
        &self.vec
    }

    fn slice_mut(&mut self) -> &mut [u8] {
        &mut self.vec
    }
}

fn __sys_call<P: AsRef<[u8]>, F: Fn(*const u8, usize, *mut u8, usize, *mut u8, *mut u8) -> i32>(
    p: P,
    sys_fn: F,
    fn_name: &'static str,
) -> RS<Vec<u8>> {
    let param = p.as_ref();
    let mut out_mem = OutMemory::default();
    let ret_value = {
        let mut out_mem_len = [0u8; size_of::<u32>()];
        let mut out_mem_id = [0u8; size_of::<u32>()];
        let n = sys_fn(
            param.as_ptr(),
            param.len(),
            out_mem.slice_mut().as_mut_ptr(),
            out_mem.slice().len(),
            out_mem_len.as_mut_ptr(),
            out_mem_id.as_mut_ptr(),
        );
        let mem_id = read_u32(&out_mem_id);
        out_mem.len = read_u32(&out_mem_len);
        if mem_id != 0 {
            // the provided memory is insufficient
            out_mem.vec.resize(out_mem.len as usize, 0);
            let size = unsafe {
                extern_c::sys_get_memory(
                    mem_id,
                    out_mem.slice_mut().as_mut_ptr(),
                    out_mem.slice().len(),
                )
            };
            if size != out_mem.len as i32 {
                panic!("output memory does not match expected size")
            }
        }
        n
    };
    if ret_value != 0 {
        return Err(m_error!(
            EC::InternalErr,
            format!("sys call {} error, return code:{}", fn_name, ret_value)
        ));
    }
    Ok(out_mem.into())
}

fn __sys_query(query_in: &[u8]) -> RS<Vec<u8>> {
    __sys_call(query_in, ___sys_query, "sys_query")
}

fn __sys_command(command_in: &[u8]) -> RS<Vec<u8>> {
    __sys_call(command_in, ___sys_command, "sys_command")
}

fn __sys_fetch(result_cursor: &[u8]) -> RS<Vec<u8>> {
    __sys_call(result_cursor, ___sys_fetch, "sys_fetch")
}

fn __sys_get(get_in: &[u8]) -> RS<Vec<u8>> {
    __sys_call(get_in, ___sys_get, "sys_get")
}

fn __sys_open(open_in: &[u8]) -> RS<Vec<u8>> {
    __sys_call(open_in, ___sys_open, "sys_open")
}

fn __sys_close(close_in: &[u8]) -> RS<Vec<u8>> {
    __sys_call(close_in, ___sys_close, "sys_close")
}

fn __sys_put(put_in: &[u8]) -> RS<Vec<u8>> {
    __sys_call(put_in, ___sys_put, "sys_put")
}

fn __sys_range(range_in: &[u8]) -> RS<Vec<u8>> {
    __sys_call(range_in, ___sys_range, "sys_range")
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
        extern_c::sys_query(
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
        extern_c::sys_command(
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
        extern_c::sys_fetch(
            param_buf_ptr,
            param_buf_len,
            out_buf_ptr,
            out_buf_len,
            out_len,
            mem_id,
        )
    }
}

fn ___sys_get(
    param_buf_ptr: *const u8,
    param_buf_len: usize,
    out_buf_ptr: *mut u8,
    out_buf_len: usize,
    out_len: *mut u8,
    mem_id: *mut u8,
) -> i32 {
    unsafe {
        extern_c::sys_get(
            param_buf_ptr,
            param_buf_len,
            out_buf_ptr,
            out_buf_len,
            out_len,
            mem_id,
        )
    }
}

fn ___sys_open(
    param_buf_ptr: *const u8,
    param_buf_len: usize,
    out_buf_ptr: *mut u8,
    out_buf_len: usize,
    out_len: *mut u8,
    mem_id: *mut u8,
) -> i32 {
    unsafe {
        extern_c::sys_open(
            param_buf_ptr,
            param_buf_len,
            out_buf_ptr,
            out_buf_len,
            out_len,
            mem_id,
        )
    }
}

fn ___sys_close(
    param_buf_ptr: *const u8,
    param_buf_len: usize,
    out_buf_ptr: *mut u8,
    out_buf_len: usize,
    out_len: *mut u8,
    mem_id: *mut u8,
) -> i32 {
    unsafe {
        extern_c::sys_close(
            param_buf_ptr,
            param_buf_len,
            out_buf_ptr,
            out_buf_len,
            out_len,
            mem_id,
        )
    }
}

fn ___sys_put(
    param_buf_ptr: *const u8,
    param_buf_len: usize,
    out_buf_ptr: *mut u8,
    out_buf_len: usize,
    out_len: *mut u8,
    mem_id: *mut u8,
) -> i32 {
    unsafe {
        extern_c::sys_put(
            param_buf_ptr,
            param_buf_len,
            out_buf_ptr,
            out_buf_len,
            out_len,
            mem_id,
        )
    }
}

fn ___sys_range(
    param_buf_ptr: *const u8,
    param_buf_len: usize,
    out_buf_ptr: *mut u8,
    out_buf_len: usize,
    out_len: *mut u8,
    mem_id: *mut u8,
) -> i32 {
    unsafe {
        extern_c::sys_range(
            param_buf_ptr,
            param_buf_len,
            out_buf_ptr,
            out_buf_len,
            out_len,
            mem_id,
        )
    }
}
