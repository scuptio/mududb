#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::src_gen::code_gen::CodeGen;
    use mudu::common::result::RS;
    use mudu::error::ErrorCode;
    use mudu_utils::this_file;

    use std::path::PathBuf;

    // Miri cannot execute FFI calls into the tree-sitter C parser, so skip this
    // test under Miri. SQL/DDL parsing is still exercised by normal `cargo test`.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_parse_mudul() {
        let r = _test_mudul();
        match r {
            Ok(_) => {}
            Err(e) => {
                if e.ec() == ErrorCode::MlParse {
                    println!("{}", e);
                }
            }
        }
    }

    fn _test_mudul() -> RS<()> {
        for text in [
            include_str!("ddl_item.sql"),
            include_str!("ddl_warehouse.sql"),
        ] {
            let result = CodeGen::generate_entity_code_from_ddl_sql(text, "Rust", true)?;
            for (name, src) in result.source_code {
                let r = syn::parse_file(&src).map(|syntax| prettyplease::unparse(&syntax));
                if r.is_err() {
                    println!("name: {}, source code : {}\n", name, src);
                    let path = PathBuf::from(this_file!())
                        .parent()
                        .unwrap()
                        .join("artifact");
                    if !path.exists() {
                        mudu_sys::fs::sync::sync_create_dir_all(&path).unwrap()
                    }
                    let path = path.join(format!("{}.rs", name));
                    mudu_sys::fs::sync::sync_write(path, src).unwrap();
                }
            }
        }
        Ok(())
    }
}
