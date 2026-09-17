// Miri cannot execute FFI calls into the tree-sitter C parser, which the mgen
// tool uses for both WIT and SQL/DDL parsing. Skip these tests under Miri; the
// tool is still exercised by normal `cargo test` runs.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::main_inner;
    use mudu::utils::json::from_json_str;
    use mudu_binding::universal::uni_schema_desc::UniSchemaDesc;
    use mudu_binding::universal::uni_type_desc::UniTypeDesc;
    use mudu_sys::fs::sync::*;
    use mudu_utils::this_file;
    use std::path::PathBuf;

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_main_inner_entity() {
        let td_folder = PathBuf::from(this_file!())
            .parent()
            .unwrap()
            .join("test_data")
            .to_str()
            .unwrap()
            .to_string();
        let tmp_folder = mudu_sys::env_var::temp_dir().to_str().unwrap().to_string();
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

        let s = sync_read_to_string(format!("{}/types.desc.json", tmp_folder)).unwrap();
        let map = from_json_str::<UniTypeDesc>(&s).unwrap();

        let s1 = sync_read_to_string(format!("{}/types.desc.json", td_folder)).unwrap();
        let map1 = from_json_str::<UniTypeDesc>(&s1).unwrap();
        println!("{:#?}", map);
        println!("{:#?}", map1);
    }
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
    #[cfg_attr(miri, ignore)]
    fn test_main_inner_message() {
        let td_folder = PathBuf::from(this_file!())
            .parent()
            .unwrap()
            .join("test_data")
            .to_str()
            .unwrap()
            .to_string();
        let tmp_folder = mudu_sys::env_var::temp_dir().to_str().unwrap().to_string();
        let wit_folder = PathBuf::from(&td_folder)
            .join("wit")
            .to_str()
            .unwrap()
            .to_string();
        for (lang, extension) in [("rust", "rs"), ("csharp", "cs")] {
            for dir_entry in sync_read_dir_entries(&wit_folder).unwrap() {
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
    #[cfg_attr(miri, ignore)]
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
        let tmp_folder = mudu_sys::env_var::temp_dir().to_str().unwrap().to_string();
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

        let src = sync_read_to_string(output).unwrap();
        assert!(src.contains("[global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]"));
        assert!(src.contains("public MuDatType()"));
        assert!(src.contains("public required string Name { get; set; }"));
        assert!(src.contains("IMessagePackFormatter<MuDatTypeParam?>"));
        assert!(src.contains("writer.WriteNil();"));
        assert!(src.contains("reader.TryReadNil()"));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_csharp_message_generation_maps_blob_to_byte_array() {
        let tmp_folder = mudu_sys::env_var::temp_dir().to_str().unwrap().to_string();
        let wit_file = PathBuf::from(&tmp_folder).join("blob-test.wit");
        let output = PathBuf::from(&tmp_folder).join("BlobTest.cs");
        sync_write(
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

        let src = sync_read_to_string(output).unwrap();
        assert!(src.contains("public required byte[] Data { get; set; }"));
        assert!(src.contains("Data = [];"));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_message_with_func_codec_flag_generates_frame_codecs() {
        let wit_file = PathBuf::from(this_file!())
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("src")
            .join("src_gen")
            .join("fixtures")
            .join("simple_func.wit");
        for (lang, ext, needles) in [
            (
                "rust",
                "rs",
                vec!["pub enum MessageKind", "pub fn encode_get_request"],
            ),
            (
                "csharp",
                "cs",
                vec!["public enum MessageKind : uint", "EncodeGetRequest"],
            ),
            (
                "assemblyscript",
                "ts",
                vec!["export enum MessageKind", "encodeGetRequest"],
            ),
        ] {
            let output =
                mudu_sys::env_var::temp_dir().join(format!("mudu_gen_tool_func_codec.{}", ext));
            let _ = sync_remove_file(&output);
            let args = vec![
                "mgen".to_string(),
                "message".to_string(),
                "-i".to_string(),
                wit_file.to_str().unwrap().to_string(),
                "-o".to_string(),
                output.to_str().unwrap().to_string(),
                "-l".to_string(),
                lang.to_string(),
                "--with-func-codec".to_string(),
            ];

            let result = main_inner(args);
            assert!(result.is_ok(), "lang {}: {:?}", lang, result.err());

            let src = sync_read_to_string(&output).unwrap();
            for needle in needles {
                assert!(src.contains(needle), "lang {}: missing {}", lang, needle);
            }
            let _ = sync_remove_file(&output);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_main_message_type_desc_matches_snapshot() {
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
        let desc_output = PathBuf::from(&tmp_folder)
            .join("mgen_wit_schema.desc.json")
            .to_str()
            .unwrap()
            .to_string();
        let args = vec![
            "mgen".to_string(),
            "message".to_string(),
            "-i".to_string(),
            wit_folder,
            "-o".to_string(),
            tmp_folder.clone(),
            "-l".to_string(),
            "csharp".to_string(),
            "--type-desc".to_string(),
            desc_output.clone(),
        ];

        let result = main_inner(args);
        assert!(result.is_ok());

        let generated = sync_read_to_string(&desc_output).unwrap();
        let expected =
            sync_read_to_string(PathBuf::from(&td_folder).join("wit-schema.desc.json")).unwrap();
        assert_eq!(generated, expected);

        let desc = from_json_str::<UniSchemaDesc>(&generated).unwrap();
        assert!(!desc.records.is_empty());
        assert!(!desc.tables.is_empty());
        // Field numbers are 1-based in declaration order.
        for record in &desc.records {
            for (i, field) in record.fields.iter().enumerate() {
                assert_eq!(field.number, (i + 1) as u32);
            }
        }
        let _ = sync_remove_file(&desc_output);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_main_message_type_desc_single_file_matches_snapshot() {
        let td_folder = PathBuf::from(this_file!())
            .parent()
            .unwrap()
            .join("test_data")
            .to_str()
            .unwrap()
            .to_string();
        let wit_file = PathBuf::from(this_file!())
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("src")
            .join("src_gen")
            .join("fixtures")
            .join("simple_func.wit");
        let desc_output = mudu_sys::env_var::temp_dir().join("mgen_simple_func.desc.json");
        let src_output = mudu_sys::env_var::temp_dir().join("mgen_simple_func_snapshot.rs");
        let _ = sync_remove_file(&desc_output);
        let args = vec![
            "mgen".to_string(),
            "message".to_string(),
            "-i".to_string(),
            wit_file.to_str().unwrap().to_string(),
            "-o".to_string(),
            src_output.to_str().unwrap().to_string(),
            "-l".to_string(),
            "rust".to_string(),
            "--with-func-codec".to_string(),
            "--type-desc".to_string(),
            desc_output.to_str().unwrap().to_string(),
        ];

        let result = main_inner(args);
        assert!(result.is_ok());

        let generated = sync_read_to_string(&desc_output).unwrap();
        let expected =
            sync_read_to_string(PathBuf::from(&td_folder).join("simple_func.desc.json")).unwrap();
        assert_eq!(generated, expected);

        let desc = from_json_str::<UniSchemaDesc>(&generated).unwrap();
        let kinds: Vec<u32> = desc.functions.iter().map(|f| f.message_kind).collect();
        assert_eq!(kinds, vec![1, 2, 3]);
        for func in &desc.functions {
            for (i, param) in func.params.iter().enumerate() {
                assert_eq!(param.number, (i + 1) as u32);
            }
        }
        let _ = sync_remove_file(&desc_output);
        let _ = sync_remove_file(&src_output);
    }
}
