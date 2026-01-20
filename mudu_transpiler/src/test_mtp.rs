#[cfg(test)]
mod tests {
    use std::env::temp_dir;
    use std::path::PathBuf;
    use mudu::this_file;
    use crate::mtp::main_inner;

    #[test]
    fn test_rust_code() {
        let test_data_pb = PathBuf::from(this_file!())
            .parent().unwrap().to_path_buf()
            .join("test_data");
        let tmp_pb = PathBuf::from(temp_dir());
        let output_path = tmp_pb.join("procedure.gen.rs").to_str().unwrap().to_string();
        let output_proc_desc_path = tmp_pb.join("procedure.desc.json").to_str().unwrap().to_string();
        let input_path = test_data_pb.join("procedure.rs")
            .to_str().unwrap().to_string();

        let type_desc_file = test_data_pb.join("types.desc.json")
            .to_str().unwrap().to_string();
        let args = vec![
            "mtp",
            "-i", input_path.as_str(),
            "-o", output_path.as_str(),
            "-m", "test",
            "-a",
            "-p", output_proc_desc_path.as_str(),
            "-t", type_desc_file.as_str(),
            "-v",
            "rust",
        ];

        let result = main_inner(args);
        assert!(result.is_ok(), "Rust code");
    }
}