#[cfg(test)]
mod test {
    use anyhow::{Context, Result};
    use chrono::prelude::*;
    use mudu::this_file;
    use std::env;
    use std::path::PathBuf;
    use thiserror::Error;
    use wasmtime::*;

    /// Custom error types for host functions
    #[derive(Error, Debug)]
    enum HostError {
        #[error("Memory access violation")]
        MemoryAccess,

        #[error("Environment variable not found")]
        EnvVarNotFound,

        #[error("Invalid operation: {0}")]
        InvalidOperation(i32),
    }

    // region: Host Function Implementations

    /// Basic export function (WASM → host)
    fn host_export(
        mut caller: Caller<'_, ()>,
        x: i32,
        y: i32,
        z_ptr: i32,
        z_len: i32,
    ) -> Result<i32, HostError> {
        let memory = get_memory(&mut caller)?;
        let data = memory.data(&caller);

        // Convert and validate pointers
        let (z_ptr, z_len) = (z_ptr as usize, z_len as usize);
        check_bounds(z_ptr, z_len, data.len())?;

        let json_str = String::from_utf8_lossy(&data[z_ptr..z_ptr + z_len]);

        // Parse and print JSON data
        match serde_json::from_str::<serde_json::Value>(&json_str) {
            Ok(data) => println!("[HOST] Received structured data:\nx={}, y={}\n{:?}", x, y, data),
            Err(_) => println!("[HOST] Received raw data: x={}, y={}, data={}", x, y, json_str),
        }

        Ok(0) // Success code
    }

    /// Logging function (WASM → host)
    fn host_log_message(
        mut caller: Caller<'_, ()>,
        ptr: i32,
        len: i32,
    ) -> Result<(), HostError> {
        let memory = get_memory(&mut caller)?;
        let data = memory.data(&caller);

        let (ptr, len) = (ptr as usize, len as usize);
        check_bounds(ptr, len, data.len())?;

        let message = String::from_utf8_lossy(&data[ptr..ptr + len]);
        println!("[WASM] {}", message);
        Ok(())
    }

    /// Math calculation (WASM → host)
    fn host_calculate(
        _caller: Caller<'_, ()>,
        a: f64,
        b: f64,
        op: i32,
    ) -> Result<f64, HostError> {
        match op {
            1 => Ok(a + b),  // Addition
            2 => Ok(a - b),  // Subtraction
            3 => Ok(a * b),  // Multiplication
            4 => if b != 0.0 { Ok(a / b) } else { Err(HostError::InvalidOperation(op)) }, // Division
            _ => Err(HostError::InvalidOperation(op)),
        }
    }

    /// Timestamp provider (WASM → host)
    fn host_current_timestamp(_caller: Caller<'_, ()>) -> i64 {
        Utc::now().timestamp()
    }

    /// Environment variable access (WASM → host)
    fn host_get_env_var(
        mut caller: Caller<'_, ()>,
        name_ptr: i32,
        name_len: i32,
        out_ptr: i32,
        out_len: i32,
    ) -> Result<i32, HostError> {
        let memory = get_memory(&mut caller)?;
        let data = memory.data_mut(&mut caller);

        // Validate all pointers
        let (name_ptr, name_len) = (name_ptr as usize, name_len as usize);
        let (out_ptr, out_len) = (out_ptr as usize, out_len as usize);
        check_bounds(name_ptr, name_len, data.len())?;
        check_bounds(out_ptr, out_len, data.len())?;

        // Read variable name
        let var_name = String::from_utf8_lossy(&data[name_ptr..name_ptr + name_len]);
        // Write variable value if exists
        if let Ok(var_value) = env::var(var_name.as_ref().to_string()) {
            let bytes = var_value.as_bytes();
            let copy_len = bytes.len().min(out_len);
            data[out_ptr..out_ptr + copy_len].copy_from_slice(&bytes[..copy_len]);
            Ok(0) // Success
        } else {
            data[0..out_len].fill(0);
            Ok(0)
        }
    }

    // endregion

    // region: Helper Functions

    /// Safely access WASM memory
    fn get_memory(caller: &mut Caller<'_, ()>) -> Result<Memory, HostError> {
        match caller.get_export("memory") {
            Some(Extern::Memory(mem)) => Ok(mem),
            _ => Err(HostError::MemoryAccess),
        }
    }

    /// Validate memory access bounds
    fn check_bounds(ptr: usize, len: usize, memory_size: usize) -> Result<(), HostError> {
        if ptr + len > memory_size {
            Err(HostError::MemoryAccess)
        } else {
            Ok(())
        }
    }

    // endregion
    #[test]
    fn test() -> Result<()> {
        // Initialize WASM engine
        let engine = Engine::default();
        let mut store = Store::new(&engine, ());

        // Configure linker with host functions
        let mut linker = Linker::new(&engine);

        // Register all host functions under "env" namespace
        linker.func_wrap("env", "basic_export",
                         |caller: Caller<'_, ()>, x: i32, y: i32, z_ptr: i32, z_len: i32|
                             host_export(caller, x, y, z_ptr, z_len).unwrap())?;
        linker.func_wrap("env", "log_message", |c: Caller<'_, ()>, p, l| host_log_message(c, p, l).unwrap())?;
        linker.func_wrap("env", "calculate", |c: Caller<'_, ()>, a, b, op| host_calculate(c, a, b, op).unwrap_or(f64::NAN))?;
        linker.func_wrap("env", "current_timestamp", host_current_timestamp)?;
        linker.func_wrap("env", "get_env_var",
                         |c: Caller<'_, ()>,
                          name_ptr: i32,
                          name_len: i32,
                          out_ptr: i32,
                          out_len: i32|
                             host_get_env_var(c, name_ptr, name_len, out_ptr, out_len).unwrap(),
        )?;

        let wasm_path = PathBuf::from(this_file!())
            .parent().unwrap().to_path_buf()
            .parent().unwrap().to_path_buf()
            .parent().unwrap().to_path_buf()
            .parent().unwrap().to_path_buf()
            .join("mudu_wasm".to_string())
            .join("wasm_module".to_string())
            .join("mudu_wasm.wasm".to_string());
        println!("wasm_path: {:?}", wasm_path);
        // Load and instantiate WASM module
        let wasm_bytes = std::fs::read(wasm_path)?;
        let module = Module::new(&engine, wasm_bytes)?;
        let instance = linker.instantiate(&mut store, &module)?;

        // Prepare input strings
        let inputs = ["Hello", "WASM"];
        let memory = instance.get_memory(&mut store, "memory")
            .context("Memory not found".to_string())?;

        // Write inputs to WASM memory
        let mut pointers = Vec::new();
        let mut offset = 0;

        for input in &inputs {
            let bytes = input.as_bytes();
            let len = bytes.len();

            // Grow memory if needed (64KB pages)
            if offset + len > memory.data_size(&store) {
                let pages_needed = ((len + 65535) / 65536) as u64;
                memory.grow(&mut store, pages_needed)?;
            }

            memory.write(&mut store, offset, bytes)?;
            pointers.push((offset as i32, len as i32));
            offset += len;
        }

        // Call WASM function
        let run_func = instance.get_typed_func::<(i32, i32, i32, i32), i32>(&mut store, "run")?;
        let (p1_ptr, p1_len) = pointers[0];
        let (p2_ptr, p2_len) = pointers[1];

        println!("[HOST] Executing WASM function...");
        match run_func.call(&mut store, (p1_ptr, p1_len, p2_ptr, p2_len))? {
            0 => println!("[HOST] Execution succeeded"),
            code => println!("[HOST] Execution failed with code: {}", code),
        }

        Ok(())
    }
}