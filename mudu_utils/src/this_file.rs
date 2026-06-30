use std::path::PathBuf;

/// Returns the absolute path of the current source file, resolved against the
/// project home directory (the first ancestor containing `.project.home`).
#[macro_export]
macro_rules! this_file {
    () => {
        $crate::this_file::__this_file(file!())
    };
}

pub fn __this_file(file: &str) -> String {
    let manifest_dir = mudu_sys::env_var::var("CARGO_MANIFEST_DIR").unwrap();
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

#[cfg(test)]
mod tests {
    use super::__this_file;

    #[test]
    fn this_file_is_non_empty() {
        let path = __this_file(file!());
        assert!(!path.is_empty());
    }

    #[test]
    fn this_file_contains_crate_and_relative_path() {
        let path = __this_file(file!());
        assert!(path.contains("mudu_utils"), "path: {path}");
        assert!(path.contains("src/this_file.rs"), "path: {path}");
    }

    #[test]
    fn this_file_is_absolute_on_unix() {
        let path = __this_file(file!());
        assert!(path.starts_with('/'), "path: {path}");
    }
}
