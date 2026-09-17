#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]
use crate::meta::_fuzz::{fuzz_printable, write_data_to_csv};
use arbitrary::Unstructured;
use std::path::PathBuf;

fn root_path() -> String {
    let _p = PathBuf::from(file!());
    let path = _p
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    path
}

fn input_output_path(name: &str) -> (String, String) {
    let path = root_path();
    let input_json = PathBuf::from(path.clone());
    let output_folder = PathBuf::from(path.clone());
    let input_json = input_json.join(format!("data/meta/{}.json", name));
    let output_folder = output_folder.join("data/meta/");
    let input_json = input_json.as_os_str().to_str().unwrap().to_string();
    let output_folder = output_folder.as_os_str().to_str().unwrap().to_string();
    (input_json, output_folder)
}

fn fuzz_gen_csv(u: &[u8], name: &str) {
    let (input_file, output_folder) = input_output_path(name);
    let mut u = Unstructured::new(u);
    fuzz_printable(input_file, output_folder, &mut u).unwrap();
}

pub fn _gen_order_csv(u: &[u8]) {
    fuzz_gen_csv(u, "oorder");
}

fn _test_gen_csv(name: &str) {
    let (input_file, output_folder) = input_output_path(name);
    write_data_to_csv(input_file, output_folder).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn root_path_is_consistent_with_source_tree() {
        let root = root_path();
        assert!(!root.is_empty());
        let manifest = env!("CARGO_MANIFEST_DIR");
        assert!(
            Path::new(manifest).ends_with(&root),
            "root_path {:?} should be a suffix of the manifest dir {:?}",
            root,
            manifest
        );
    }

    #[test]
    fn input_output_path_builds_meta_paths() {
        let name = "oorder";
        let (input, output) = input_output_path(name);
        let root = root_path();

        assert!(input.starts_with(&root));
        assert!(input.ends_with(&format!("data/meta/{}.json", name)));
        assert_eq!(Path::new(&input).file_name().unwrap(), "oorder.json");

        assert!(output.starts_with(&root));
        assert!(output.ends_with("data/meta/"));
    }
}
