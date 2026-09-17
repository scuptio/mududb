use std::path::PathBuf;

/// Returns the absolute path of the current source file.
///
/// `file!()` is relative to the directory `rustc` was invoked from, which may
/// be the crate root, a group workspace root (`crates/<group>`), or the
/// repository root. The path is therefore resolved against the caller crate's
/// `CARGO_MANIFEST_DIR` by stripping leading components until the remainder
/// exists on disk. As a fallback for files that do not exist (yet), the path
/// is joined onto the project home directory (the first ancestor of
/// `CARGO_MANIFEST_DIR` containing `.project.home`).
#[macro_export]
macro_rules! this_file {
    () => {
        $crate::this_file::__this_file(file!())
    };
}

pub fn __this_file(file: &str) -> String {
    let file_path = PathBuf::from(file);
    if file_path.is_absolute() {
        return file.to_string();
    }

    let manifest_dir = mudu_sys::env_var::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest_dir_path_buf = PathBuf::from(&manifest_dir);

    // The file always lives inside the caller crate; strip leading components
    // (e.g. `mudu_kernel/` or `crates/db-kernel/mudu_kernel/`) until the
    // remainder resolves to an existing path under the manifest directory.
    let components: Vec<_> = file_path.components().collect();
    for i in 0..components.len() {
        let rel: PathBuf = components[i..].iter().collect();
        let candidate = manifest_dir_path_buf.join(rel);
        if candidate.exists() {
            return candidate
                .to_str()
                .map(|s| s.to_string())
                .unwrap_or_default();
        }
    }

    // Fallback: resolve against the project home directory.
    let mut project_home = manifest_dir_path_buf;
    let home_path = loop {
        if project_home.join(".project.home").exists() {
            break project_home;
        } else if !project_home.pop() {
            break PathBuf::from(&manifest_dir);
        }
    };
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
