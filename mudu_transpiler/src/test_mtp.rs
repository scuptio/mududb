//! Integration tests for the Rust front-end of the Mudu transpiler.
#![allow(missing_docs)]

#[cfg(test)]
mod tests {
    use crate::mtp::main_inner;
    use mudu_utils::this_file;
    use std::error::Error;
    use std::path::PathBuf;

    // The Rust transpiler uses tree-sitter, which calls a native C grammar that
    // Miri does not support.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn test_rust_code() -> Result<(), Box<dyn Error>> {
        let test_data_pb = PathBuf::from(this_file!())
            .parent()
            .ok_or("test file has no parent directory")?
            .to_path_buf()
            .join("test_data");
        let tmp_pb = mudu_sys::env_var::temp_dir();
        let output_path = tmp_pb
            .join("procedure.gen.rs")
            .to_str()
            .ok_or("invalid UTF-8 in output path")?
            .to_string();
        let output_proc_desc_path = tmp_pb
            .join("procedure.desc.json")
            .to_str()
            .ok_or("invalid UTF-8 in desc path")?
            .to_string();
        let input_path = test_data_pb
            .join("procedure.rs")
            .to_str()
            .ok_or("invalid UTF-8 in input path")?
            .to_string();

        let type_desc_file = test_data_pb
            .join("types.desc.json")
            .to_str()
            .ok_or("invalid UTF-8 in type desc path")?
            .to_string();
        let args = vec![
            "mtp",
            "-i",
            input_path.as_str(),
            "-o",
            output_path.as_str(),
            "-m",
            "test",
            "-a",
            "-p",
            output_proc_desc_path.as_str(),
            "-t",
            type_desc_file.as_str(),
            "-v",
            "rust",
        ];

        let result = main_inner(args);
        assert!(result.is_ok(), "Rust code");
        Ok(())
    }
}
