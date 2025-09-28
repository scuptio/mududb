use crate::procedure::procedure::Procedure;
use crate::procedure::wasi_context::WasiContext;
use crate::resolver::schema_mgr::SchemaMgr;
use crate::resolver::sql_resolver::SQLResolver;
use crate::runtime::procedure_invoke::ProcedureInvoke;
use mudu::common::endian::write_u32;
use mudu::common::result::RS;
use mudu::common::result_of::rs_option;
use mudu::common::serde_utils::{deserialize_sized_from, serialize_sized_to, serialize_sized_to_vec};
use mudu::common::xid::XID;
use mudu::database::err_no;
use mudu::database::sql::Context;
use mudu::database::v2h_param::{
    CommandIn,
    CommandOut,
    QueryIn,
    QueryResult,
    ResultCursor, ResultRow,
};
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::procedure::proc_desc::ProcDesc;
use mudu::procedure::proc_param::ProcParam;
use mudu::procedure::proc_result::ProcResult;
use mudu::tuple::datum::DatumDyn;
use serde::de::DeserializeOwned;
use serde::Serialize;
use sql_parser::ast::parser::SQLParser;
use std::cmp::min;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use wasmtime::{AsContext, Caller, Engine, Extern, Linker, Memory, Module};
use wasmtime_wasi::WasiCtxBuilder;

const MP_BYTE_CODE_EXTENSION: &str = "wasm";
const MP_DESC_EXTENSION: &str = "toml";

pub struct RuntimeSimple {
    engine: Engine,
    context: WasiContext,
    procedure: HashMap<String, Procedure>,
    parser: SQLParser,
    resolver: SQLResolver,
    linker: Linker<WasiContext>,
}


fn extract_file_name_without_extension(path: &Path, extension: &str) -> String {
    if let Some(file_name_os) = path.file_stem() {
        if let Some(file_name) = file_name_os.to_str() {
            return file_name.to_string();
        }
    }

    // If unable to extract the file name
    path.to_string_lossy()
        .replace(extension, "")
        .trim_end_matches('.')
        .to_string()
}

fn read_byte_code_and_proc_desc_files<P: AsRef<Path>>(
    dir_path: P,
    module: &mut HashMap<String, Vec<u8>>,
    proc_desc: &mut HashMap<String, ProcDesc>,
) -> RS<()> {
    let current_dir = dir_path.as_ref();
    for entry in fs::read_dir(&current_dir)
        .map_err(|e| {
            m_error!(EC::MuduError, format!("read directory {:?} error", current_dir), e)
        })?
    {
        let entry = entry
            .map_err(|e| {
                m_error!(EC::MuduError, "entry  error", e)
            })?;
        let path = entry.path();

        // check file name
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext.to_ascii_lowercase() == MP_BYTE_CODE_EXTENSION {
                    let content = fs::read(&path)
                        .map_err(|e| {
                            m_error!(EC::MuduError, format!("read file {:?} error", path), e)
                        })?;
                    let name = extract_file_name_without_extension(&path, MP_BYTE_CODE_EXTENSION);
                    module.insert(
                        name,
                        content,
                    );
                } else if ext.to_ascii_lowercase() == MP_DESC_EXTENSION {
                    let desc = ProcDesc::from_path(path)?;
                    proc_desc.insert(desc.proc_name().clone(), desc);
                }
            }
        }
    }

    Ok(())
}

impl RuntimeSimple {
    pub fn new(schema_mgr: SchemaMgr) -> RuntimeSimple {
        let engine = Engine::default();
        let context = Self::build_context();
        // Configure linker with host functions
        let linker = Linker::new(&engine);
        let parser = SQLParser::new();
        let resolver = SQLResolver::new(schema_mgr);
        Self {
            engine,
            context,
            procedure: Default::default(),
            parser,
            resolver,
            linker,
        }
    }

    fn build_context() -> WasiContext {
        let wasi = WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_args()
            .build_p1();
        let context = WasiContext::new(wasi);
        context
    }
    pub fn initialized<P: AsRef<Path>>(&mut self, byte_code_folder: P) -> RS<()> {
        Self::register_core_sys_call(&mut self.linker)?;
        let mut map_byte_code = HashMap::new();
        let mut map_proc_desc = HashMap::new();
        read_byte_code_and_proc_desc_files(
            byte_code_folder.as_ref(),
            &mut map_byte_code,
            &mut map_proc_desc,
        )?;

        wasmtime_wasi::preview1::add_to_linker_sync(&mut self.linker, |ctx|
            {
                ctx.wasi_mut()
            })
            .map_err(|e| {
                m_error!(EC::MuduError, "wasmtime_wasi add_to_linker_sync error", e)
            })?;
        for (name, proc_desc) in map_proc_desc {
            let opt_byte_code = map_byte_code.get(proc_desc.module_name());
            match opt_byte_code {
                Some(byte_code) => {
                    let module = Module::from_binary(&self.engine, byte_code)
                        .map_err(|e| {
                            m_error!(EC::MuduError, format!("build module {} from binary error", name), e)
                        })?;
                    let instance_pre = self.linker
                        .instantiate_pre(&module)
                        .map_err(|e| {
                            m_error!(EC::MuduError, format!("instantiate module {} error", name), e)
                        })?;

                    self.procedure.insert(
                        name,
                        Procedure::new(proc_desc, instance_pre),
                    );
                }
                None => {
                    panic!("module name {:?} not found", proc_desc.module_name());
                }
            }
        }
        Ok(())
    }


    fn register_core_sys_call(linker: &mut Linker<WasiContext>) -> RS<()> {
        let module_name = "env";
        linker.func_wrap(
            module_name, "query",
            |caller: Caller<'_, WasiContext>,
             param_buf_ptr: u32,
             param_buf_len: u32,
             out_buf_ptr: u32,
             out_buf_len: u32,
             out_mem_ptr: u32,
             out_mem_len: u32,
            | -> i32 {
                kernel_query(
                    caller,
                    param_buf_ptr,
                    param_buf_len,
                    out_buf_ptr,
                    out_buf_len,
                    out_mem_ptr,
                    out_mem_len,
                )
            },
        ).map_err(|e| { m_error!(EC::MuduError, "", e) })?;

        linker.func_wrap(
            module_name, "sys_command",
            |caller: Caller<'_, WasiContext>,
             param_buf_ptr: u32,
             param_buf_len: u32,
             out_buf_ptr: u32,
             out_buf_len: u32,
             out_mem_ptr: u32,
             out_mem_len: u32,
            | -> i32 {
                kernel_command(
                    caller,
                    param_buf_ptr,
                    param_buf_len,
                    out_buf_ptr,
                    out_buf_len,
                    out_mem_ptr,
                    out_mem_len,
                )
            },
        ).map_err(|e| { m_error!(EC::MuduError, "", e) })?;

        linker.func_wrap(
            module_name, "sys_fetch",
            |caller: Caller<'_, WasiContext>,
             param_buf_ptr: u32,
             param_buf_len: u32,
             out_buf_ptr: u32,
             out_buf_len: u32,
             out_mem_ptr: u32,
             out_mem_len: u32,
            | -> i32 {
                kernel_fetch(
                    caller,
                    param_buf_ptr,
                    param_buf_len,
                    out_buf_ptr,
                    out_buf_len,
                    out_mem_ptr,
                    out_mem_len,
                )
            },
        ).map_err(|e| { m_error!(EC::MuduError, "", e) })?;

        linker.func_wrap(
            module_name, "sys_get_memory",
            |caller: Caller<'_, WasiContext>,
             mem_id: u32,
             out_buf_ptr: u32,
             out_buf_len: u32| -> i32 {
                kernel_get_memory(
                    caller,
                    mem_id,
                    out_buf_ptr,
                    out_buf_len,
                )
            },
        ).map_err(|e| { m_error!(EC::MuduError, "", e) })?;

        Ok(())
    }

    pub fn describe(&self, name: &String) -> RS<Arc<ProcDesc>> {
        let opt_procedure = self.procedure.get(name);
        let procedure: &Procedure = rs_option(
            opt_procedure,
            &format!("no procedure named {}", name))?;
        Ok(procedure.desc())
    }

    pub fn invoke_procedure(&self, name: &String, param: ProcParam) -> RS<ProcResult> {
        let opt_procedure = self.procedure.get(name);
        let procedure: &Procedure = rs_option(
            opt_procedure,
            &format!("no procedure named {}", name))?;
        let name = format!("{}{}", mudu::procedure::proc::MUDU_PROC_PREFIX, procedure.name());

        let result = ProcedureInvoke::call(
            Self::build_context(),
            procedure.instance(),
            Default::default(),
            name,
            param,
        )?;
        Ok(result)
    }
}


fn get_memory(caller: &mut Caller<'_, WasiContext>) -> RS<Memory> {
    match caller.get_export("memory") {
        Some(Extern::Memory(mem)) => Ok(mem),
        _ => Err(m_error!(EC::MuduError, "get memory export error")),
    }
}

pub fn kernel_query(
    caller: Caller<'_, WasiContext>,
    param_buf_ptr: u32,
    param_buf_len: u32,
    out_buf_ptr: u32,
    out_buf_len: u32,
    out_mem_ptr: u32,
    out_mem_len: u32,
) -> i32 {
    handle_vm_invoke_host::<QueryIn, QueryResult, _>(
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

pub fn kernel_fetch(
    caller: Caller<'_, WasiContext>,
    param_buf_ptr: u32, param_buf_len: u32,
    output_buf_ptr: u32, output_buf_len: u32,
    out_mem_ptr: u32,
    out_mem_len: u32,
) -> i32 {
    handle_vm_invoke_host::<ResultCursor, ResultRow, _>(
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


pub fn kernel_command(
    caller: Caller<'_, WasiContext>,
    param_buf_ptr: u32, param_buf_len: u32,
    output_buf_ptr: u32, output_buf_len: u32,
    out_mem_ptr: u32,
    out_mem_len: u32,
) -> i32 {
    handle_vm_invoke_host::<CommandIn, CommandOut, _>(
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

fn query_gut(
    query_in: &QueryIn,
) -> RS<QueryResult> {
    let xid = query_in.xid();
    let context = get_context(xid)?;
    let param: Vec<&dyn DatumDyn> = query_in
        .param()
        .iter()
        .map(|e| { todo!() })
        .collect();
    let result = context.query_raw(
        &query_in.sql(),
        &param,
    )?;
    let rs = context.cache_result(result)?;
    Ok(rs)
}

fn query_fetch_gut(query_cursor: &ResultCursor) -> RS<ResultRow> {
    let context = get_context(query_cursor.xid())?;
    let opt_tuple = context.query_next()?;
    Ok(ResultRow::new(opt_tuple))
}

fn command_gut(
    command_in: &CommandIn
) -> RS<CommandOut> {
    let xid = command_in.xid();
    let context = get_context(xid)?;
    let param: Vec<&dyn DatumDyn> = command_in
        .param()
        .iter()
        .map(|e| { todo!() })
        .collect();
    let affected_rows = context.command(
        &command_in.sql(),
        &param,
    )?;
    Ok(CommandOut::new(affected_rows as _))
}

fn get_context(xid: XID) -> RS<Context> {
    let opt = Context::context(xid);
    let context = rs_option(opt, &format!("no such transaction {}", xid))?;
    Ok(context)
}


fn handle_vm_invoke_host_gut<
    D: DeserializeOwned + 'static,
    S: Serialize + 'static,
    F: Fn(&D) -> RS<S> + 'static,
>(
    caller: Caller<'_, WasiContext>,
    param_buf_ptr: u32, param_buf_len: u32,
    output_buf_ptr: u32, output_buf_len: u32,
    out_len_ptr: u32,
    mem_id_ptr: u32,
    f: F,
) -> RS<bool> {
    let context = caller.data().context_ptr();
    let mut caller = caller;

    let memory = get_memory(&mut caller).unwrap();
    let input = {
        let buf = memory.data(&caller);
        let _buf_ptr = param_buf_ptr as usize;
        let _buf_len = param_buf_len as usize;
        check_bounds(_buf_ptr, _buf_len, buf.len())?;
        let in_param = &buf[_buf_ptr.._buf_ptr + _buf_len];

        let (d, _size): (D, usize) = deserialize_sized_from(in_param)?;
        d
    };
    let result = f(&input)?;
    let ok = {
        let buf = memory.data_mut(&mut caller);
        let _out_buf_ptr = output_buf_ptr as usize;
        let _out_buf_len = output_buf_len as usize;
        check_bounds(_out_buf_ptr, _out_buf_len, buf.len())?;
        let out_param = &mut buf[_out_buf_ptr.._out_buf_ptr + _out_buf_len];
        let (ok, size) = serialize_sized_to(&result, out_param)?;
        let _out_len_ptr = out_len_ptr as usize;
        let _size = size as u32;
        let _mem_id_ptr = mem_id_ptr as usize;

        // write the expected output buffer size
        check_bounds(_out_len_ptr, size_of::<u32>(), buf.len())?;
        write_u32(&mut buf[_out_len_ptr.._out_len_ptr + size_of::<u32>()], _size);
        if !ok {
            let vec = serialize_sized_to_vec(&result)?;
            let context_ref = unsafe { &*context };
            let n = context_ref.add_memory(vec);
            check_bounds(_mem_id_ptr, size_of::<u32>(), buf.len())?;
            write_u32(&mut buf[_out_len_ptr.._out_len_ptr + size_of::<u32>()], n);
        }
        ok
    };

    Ok(ok)
}

fn handle_vm_invoke_host<
    D: DeserializeOwned + 'static,
    S: Serialize + 'static,
    F: Fn(&D) -> RS<S> + 'static,
>(
    caller: Caller<'_, WasiContext>,
    param_buf_ptr: u32, param_buf_len: u32,
    output_buf_ptr: u32, output_buf_len: u32,
    out_len_ptr: u32,
    mem_id_ptr: u32,
    f: F,
) -> i32 {
    let r = handle_vm_invoke_host_gut(
        caller,
        param_buf_ptr, param_buf_len,
        output_buf_ptr, output_buf_len,
        out_len_ptr,
        mem_id_ptr,
        f,
    );
    match r {
        Ok(ok) => {
            if ok {
                err_no::EN_OK
            } else {
                err_no::EN_INSUFFICIENT_OUTPUT_BUFFER_SPACE
            }
        }
        Err(e) => {
            err_no::EN_ENCODE_RESULT_ERROR
        }
    }
}


// region: Helper Functions

/// Safely access WASM memory
fn get_input(caller: &mut Caller<'_, WasiContext>) -> RS<Memory> {
    match caller.get_export("input_mem") {
        Some(Extern::Memory(mem)) => Ok(mem),
        _ => Err(m_error!(EC::WASMMemoryAccessError, "get input error")),
    }
}


fn get_output(caller: &mut Caller<'_, WasiContext>) -> RS<Memory> {
    match caller.get_export("output") {
        Some(Extern::Memory(mem)) => Ok(mem),
        _ => Err(m_error!(EC::WASMMemoryAccessError, "get input error")),
    }
}


/// Validate memory access bounds
fn check_bounds(ptr: usize, len: usize, memory_size: usize) -> RS<()> {
    if ptr + len > memory_size {
        Err(m_error!(EC::WASMMemoryAccessError, "memory bound error"))
    } else {
        Ok(())
    }
}


#[cfg(test)]
mod test {
    use crate::resolver::schema_mgr::SchemaMgr;
    use crate::runtime::runtime_simple::RuntimeSimple;
    use crate::runtime::test_wasm_mod_path::wasm_mod_path;
    use mudu::procedure::proc_param::ProcParam;
    use mudu::tuple::rs_tuple_datum::RsTupleDatum;

    ///
    /// See proc function definition [proc](mudu_wasm/src/wasm/proc.rs#L5)。
    ///
    #[test]
    fn test_runtime_pg_simple() {
        let schema_mgr = SchemaMgr::new_empty();
        let mut runtime = RuntimeSimple::new(schema_mgr);

        let wasm_path = wasm_mod_path();

        runtime.initialized(wasm_path).unwrap();
        let tuple = (1i32, 100i64, "string argument".to_string());
        let desc = <(i32, i64, String)>::tuple_desc_static();
        let params = ProcParam::from_tuple(0, tuple, &desc).unwrap();
        let proc_result = runtime.invoke_procedure(&"proc".to_string(), params).unwrap();
        let result = proc_result.to::<(i32, String)>(&<(i32, String)>::tuple_desc_static()).unwrap();
        println!("result: {:?}", result);
    }
}
