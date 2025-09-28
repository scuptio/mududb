// Import host-provided functions
unsafe extern "C" {
    // select SQL
    pub fn query(
        param_buf_ptr: *const u8, param_buf_len: usize,
        output_buf_ptr: *mut u8, output_buf_len: usize,
        output_len_ptr: *mut u64,
    ) -> i32;


    // fetch next batch of rows of a select result
    pub fn query_fetch_next(
        param_buf_ptr: *const u8, param_buf_len: usize,
        output_buf_ptr: *mut u8, output_buf_len: usize,
        output_len_ptr: *mut u64,
    ) -> i32;

    // insert/delete/update SQL
    pub fn command(
        param_buf_ptr: *const u8, param_buf_len: usize,
        output_buf_ptr: *mut u8, output_buf_len: usize,
        output_len_ptr: *mut u64,
    ) -> i32;

    pub fn host_realloc() -> i32;
}
