#[cfg(test)]
mod tests {
    use crate::main_inner;
    use mudu::this_file;
    use std::path::PathBuf;

    #[test]
    fn test_main_message_by_folder() {
        let td_folder = PathBuf::from(this_file!())
            .parent()
            .unwrap()
            .join("test_data")
            .to_str()
            .unwrap()
            .to_string();
        let tmp_folder = std::env::temp_dir().to_str().unwrap().to_string();
        let wit_folder = PathBuf::from(&td_folder)
            .join("wit-schema")
            .to_str()
            .unwrap()
            .to_string();
        for lang in ["csharp"] {
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
