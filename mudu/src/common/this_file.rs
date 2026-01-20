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

    let mut project_home = manifest_dir_path_buf;
    let home_path = loop {
        if project_home.join(".project.home").exists() {
            break project_home;
        } else {
            project_home.pop();
        }
    };
    let file_path = PathBuf::from(file);
    let path = home_path.join(file_path);
    path.to_str()
        .map(|s| s.to_string())
        .unwrap_or(String::new())
}
