// Miri cannot execute FFI calls into the tree-sitter C parser, which the mgen
// tool uses for WIT parsing. Skip this test under Miri; it is still exercised
// by normal `cargo test` runs.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::main_inner;
    use mudu_utils::this_file;
    use std::path::PathBuf;

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_main_message_by_folder() {
        let td_folder = PathBuf::from(this_file!())
            .parent()
            .unwrap()
            .join("test_data")
            .to_str()
            .unwrap()
            .to_string();
        let tmp_folder = mudu_sys::env_var::temp_dir().to_str().unwrap().to_string();
        let wit_folder = PathBuf::from(&td_folder)
            .join("wit-schema")
            .to_str()
            .unwrap()
            .to_string();
        {
            let lang = "csharp";
            let output = tmp_folder.clone();
            let args = vec![
                "mgen".to_string(),
                "message".to_string(),
                "-i".to_string(),
                wit_folder.clone(),
                "-o".to_string(),
                output.clone(),
                "-l".to_string(),
                lang.to_string(),
            ];

            let result = main_inner(args);
            assert!(result.is_ok());
        }
    }
}
