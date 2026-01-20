use crate::interface::kernel;

pub fn host_query(query_in:Vec<u8>) -> Vec<u8> {
    kernel::query_internal(&query_in)
}

pub fn host_command(command_in: Vec<u8>) -> Vec<u8> {
    kernel::command_internal(&command_in)
}

pub fn host_fetch(result_cursor: Vec<u8>) -> Vec<u8> {
    kernel::fetch_internal(&result_cursor)
}
