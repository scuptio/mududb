use anyhow::anyhow;
use anyhow::Result;
#[cfg(target_arch = "wasm32")]
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use std::slice;

// Import host-provided functions
unsafe extern "C" {
    /// Basic export function
    pub fn basic_export(x: i32, y: i32, z_ptr: *const u8, z_len: usize) -> i32;

    /// Logging function
    pub fn log_message(ptr: *const u8, len: usize);

    /// Math calculation
    pub fn calculate(a: f64, b: f64, op: i32) -> f64;

    /// Timestamp provider
    pub fn current_timestamp() -> i64;

    /// Environment variable access
    pub fn get_env_var(ptr: *const u8, len: usize, out_ptr: *mut u8, out_len: usize) -> i32;
}

/// Exported WASM function (WASM → host)
#[no_mangle]
#[cfg(target_arch = "wasm32")]
pub extern "C" fn run(p1_ptr: *const u8, p1_len: usize, p2_ptr: *const u8, p2_len: usize) -> i32 {
    match _run(p1_ptr, p1_len, p2_ptr, p2_len) {
        Ok(_) => 0, // Success code
        Err(e) => {
            // Log errors through host
            let err_msg = format!("Error in run: {}", e);
            unsafe {
                log_message(err_msg.as_ptr(), err_msg.len());
            }
            1 // Error code
        }
    }
}

/// Core business logic
#[cfg(target_arch = "wasm32")]
fn _run(p1_ptr: *const u8, p1_len: usize, p2_ptr: *const u8, p2_len: usize) -> Result<()> {
    // SAFETY: Convert raw pointers to strings with bounds checking
    let p1 = unsafe {
        let slice = slice::from_raw_parts(p1_ptr, p1_len);
        str::from_utf8(slice)?.to_string()
    };

    let p2 = unsafe {
        let slice = slice::from_raw_parts(p2_ptr, p2_len);
        str::from_utf8(slice)?.to_string()
    };

    // Demonstrate host logging
    unsafe {
        let msg = format!("[WASM] Received params: p1='{}', p2='{}'", p1, p2);
        log_message(msg.as_ptr(), msg.len());
    }

    // Use host timestamp function
    let timestamp = unsafe { current_timestamp() };
    unsafe {
        let msg = format!("[WASM] Current timestamp: {}", timestamp);
        log_message(msg.as_ptr(), msg.len());
    }

    // Use host calculation function (1 = addition)
    let result = unsafe { calculate(10.5, 2.3, 1) };
    unsafe {
        let msg = format!("[WASM] Calculation result: 10.5 + 2.3 = {}", result);
        log_message(msg.as_ptr(), msg.len());
    }

    // Get environment variable through host
    let env_var_name = "HOME";
    let mut env_var_buf = [0u8; 256];
    let status = unsafe {
        get_env_var(
            env_var_name.as_ptr(),
            env_var_name.len(),
            env_var_buf.as_mut_ptr(),
            env_var_buf.len(),
        )
    };

    if status == 0 {
        let env_var = String::from_utf8_lossy(&env_var_buf);
        unsafe {
            let msg = format!("[WASM] {} = {}", env_var_name, env_var);
            log_message(msg.as_ptr(), msg.len());
        }
    }

    // Prepare complex data for host (using JSON)
    #[derive(Serialize, Deserialize)]
    struct ProcessedData {
        input1: String,
        input2: String,
        result: f64,
        timestamp: i64,
    }

    let data = ProcessedData {
        input1: p1,
        input2: p2,
        result,
        timestamp,
    };

    let json = serde_json::to_string(&data)?;

    // Call host export function with JSON data
    let status = unsafe {
        basic_export(
            42,            // Example integer param
            100,           // Example integer param
            json.as_ptr(), // JSON data pointer
            json.len(),    // JSON data length
        )
    };

    if status != 0 {
        return Err(anyhow!("Host export failed with code: {}", status));
    }

    Ok(())
}
