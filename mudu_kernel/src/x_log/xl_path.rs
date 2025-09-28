use std::path::PathBuf;

pub fn xl_file_path(folder: &String, name: &String, ext: &String, no: u32) -> PathBuf {
    let path = PathBuf::from(folder);
    path.join(format_xl_file_name(name, ext, no))
}

pub fn format_xl_file_name(name: &String, ext: &String, no: u32) -> String {
    format!("{}_{}.{}", name, no, ext)
}
