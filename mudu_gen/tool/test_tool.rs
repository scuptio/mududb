#[cfg(test)]
mod tests {
    use crate::main_inner;
    use mudu::this_file;
    use mudu::utils::json::from_json_str;
    use mudu_binding::universal::uni_type_desc::UniTypeDesc;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test_main_inner_entity() {
        let td_folder = PathBuf::from(this_file!())
            .parent()
            .unwrap()
            .join("test_data")
            .to_str()
            .unwrap()
            .to_string();
        let tmp_folder = std::env::temp_dir().to_str().unwrap().to_string();
        let args = vec![
            "mgen".to_string(),
            "entity".to_string(),
            "-i".to_string(),
            format!("{}/sql/ddl.sql", td_folder),
            format!("{}/sql/type.sql", td_folder),
            "-o".to_string(),
            tmp_folder.to_string(),
            "-t".to_string(),
            format!("{}/types.desc.json", tmp_folder),
            "-l".to_string(),
            "rust".to_string(),
        ];

        let result = main_inner(args);

        assert!(result.is_ok());

        let s = fs::read_to_string(format!("{}/types.desc.json", tmp_folder)).unwrap();
        let map = from_json_str::<UniTypeDesc>(&s).unwrap();

        let s1 = fs::read_to_string(format!("{}/types.desc.json", td_folder)).unwrap();
        let map1 = from_json_str::<UniTypeDesc>(&s1).unwrap();
        println!("{:#?}", map);
        println!("{:#?}", map1);
    }
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
            .join("wit")
            .to_str()
            .unwrap()
            .to_string();
        for lang in ["rust", "csharp"] {
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

    #[test]
    fn test_main_inner_message() {
        let td_folder = PathBuf::from(this_file!())
            .parent()
            .unwrap()
            .join("test_data")
            .to_str()
            .unwrap()
            .to_string();
        let tmp_folder = std::env::temp_dir().to_str().unwrap().to_string();
        let wit_folder = PathBuf::from(&td_folder)
            .join("wit")
            .to_str()
            .unwrap()
            .to_string();
        for (lang, extension) in [("rust", "rs"), ("csharp", "cs")] {
            for r in fs::read_dir(&wit_folder).unwrap() {
                let dir_entry = r.unwrap();
                let sterm = dir_entry
                    .path()
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string();
                let wit_file = dir_entry.path().to_str().unwrap().to_string();
                let output = PathBuf::from(&tmp_folder)
                    .join(format!("{}.{}", sterm, extension))
                    .to_str()
                    .unwrap()
                    .to_string();
                let args = vec![
                    "mgen".to_string(),
                    "message".to_string(),
                    "-i".to_string(),
                    wit_file,
                    "-o".to_string(),
                    output,
                    "-l".to_string(),
                    lang.to_string(),
                ];

                let result = main_inner(args);
                assert!(result.is_ok());
            }
        }
    }

    #[test]
    fn test_csharp_message_generation_has_default_ctor_and_nullable_formatter() {
        let contract_wit = PathBuf::from(this_file!())
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("src")
            .join("src_gen")
            .join("contract.wit")
            .to_str()
            .unwrap()
            .to_string();
        let tmp_folder = std::env::temp_dir().to_str().unwrap().to_string();
        let output = PathBuf::from(&tmp_folder)
            .join("Contract.cs")
            .to_str()
            .unwrap()
            .to_string();
        let args = vec![
            "mgen".to_string(),
            "message".to_string(),
            "-i".to_string(),
            contract_wit,
            "-o".to_string(),
            output.clone(),
            "-l".to_string(),
            "csharp".to_string(),
        ];

        let result = main_inner(args);
        assert!(result.is_ok());

        let src = fs::read_to_string(output).unwrap();
        assert!(src.contains("[global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]"));
        assert!(src.contains("public MuDatType()"));
        assert!(src.contains("public required string Name { get; set; }"));
        assert!(src.contains("IMessagePackFormatter<MuDatTypeParamArray?>"));
        assert!(src.contains("writer.WriteNil();"));
        assert!(src.contains("reader.TryReadNil()"));
    }

    #[test]
    fn test_csharp_message_generation_maps_blob_to_byte_array() {
        let tmp_folder = std::env::temp_dir().to_str().unwrap().to_string();
        let wit_file = PathBuf::from(&tmp_folder).join("blob-test.wit");
        let output = PathBuf::from(&tmp_folder).join("BlobTest.cs");
        fs::write(
            &wit_file,
            r#"
package test:blob;

interface blob-test {
    record payload {
        data: blob,
    }
}
"#,
        )
        .unwrap();

        let args = vec![
            "mgen".to_string(),
            "message".to_string(),
            "-i".to_string(),
            wit_file.to_str().unwrap().to_string(),
            "-o".to_string(),
            output.to_str().unwrap().to_string(),
            "-l".to_string(),
            "csharp".to_string(),
        ];

        let result = main_inner(args);
        assert!(result.is_ok());

        let src = fs::read_to_string(output).unwrap();
        assert!(src.contains("public required byte[] Data { get; set; }"));
        assert!(src.contains("Data = [];"));
    }
}
