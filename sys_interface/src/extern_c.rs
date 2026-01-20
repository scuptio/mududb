// Declares kernel system calls.
// These functions are unsafe because they interact with raw pointers and have
// contracts that must be upheld by the caller to avoid undefined behavior.

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    // Executes a SQL command with serialized parameters.
    //
    // # Arguments
    // * `param_buf_ptr` - Pointer to read-only buffer containing serialized input parameters
    // * `param_buf_len` - Length of the input parameter buffer in bytes
    // * `out_buf_ptr`   - Pointer to mutable output buffer for serialized results
    // * `out_buf_len`   - Capacity of the output buffer in bytes
    // * `out_len`       - Output parameter for minimal required buffer size (at least 4 byte, mut pointer)
    // * `mem_id`        - Output parameter for memory identifier when buffer is insufficient (at least 4 bytes, mut pointer)
    //
    // # Returns
    // System-specific status code (non-zero indicates error)
    //
    // # Safety
    // All pointer parameters must be valid:
    // - Input buffers must contain properly serialized data
    // - Output pointers must point to allocated memory of correct size
    // - The function may write to output buffers and output parameters
    pub fn sys_command(
        param_buf_ptr: *const u8,
        param_buf_len: usize,
        out_buf_ptr: *mut u8,
        out_buf_len: usize,
        out_len: *mut u8,
        mem_id: *mut u8,
    ) -> i32;

    // Similar to sys_command but semantically used for query operations.
    // Shares the same signature and safety requirements as `sys_command`.
    pub fn sys_query(
        param_buf_ptr: *const u8,
        param_buf_len: usize,
        out_buf_ptr: *mut u8,
        out_buf_len: usize,
        out_len: *mut u8,
        mem_id: *mut u8,
    ) -> i32;

    // Similar to sys_command but semantically used for fetch operations.
    // Shares the same signature and safety requirements as `sys_command`.
    pub fn sys_fetch(
        param_buf_ptr: *const u8,
        param_buf_len: usize,
        out_buf_ptr: *mut u8,
        out_buf_len: usize,
        out_len: *mut u8,
        mem_id: *mut u8,
    ) -> i32;

    // Retrieves memory content using a previously obtained memory identifier.
    //
    // # Arguments
    // * `mem_id`       - Memory identifier obtained from failed sys_command/sys_query/sys_fetch call
    // * `out_buf_ptr`  - Pointer to buffer that will receive the memory contents
    // * `out_buf_len`  - Size of the output buffer in bytes
    //
    // # Safety
    // - `mem_id` must be a valid identifier obtained from a prior system call
    // - `out_buf_ptr` must point to valid memory of at least `out_buf_len` bytes obtained from
    //    sys_command/sys_query/sys_fetch.
    // - The kernel may write up to `out_buf_len` bytes to the output buffer
    pub fn sys_get_memory(
        mem_id: u32,
        out_buf_ptr: *mut u8,
        out_buf_len: usize,
    ) -> i32;
}
