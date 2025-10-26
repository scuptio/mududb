use std::path::PathBuf;

#[macro_export]
macro_rules! this_file {
    () => {
        $crate::common::this_file::__this_file(file!())
    };
}

pub fn __this_file(file: &str) -> String {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok().unwrap();
    let manifest_dir_path_buf = PathBuf::from(&manifest_dir);
    let manifest_dir = manifest_dir_path_buf
        .parent()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let file_path = PathBuf::from(file);
    let path = PathBuf::from(manifest_dir).join(file_path);
    path.to_str()
        .map(|s| s.to_string())
        .unwrap_or(String::new())
}
