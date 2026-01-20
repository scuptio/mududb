#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use rust_format::Formatter;
    use mudu::common::result::RS;
    use mudu::error::ec::EC;
    use mudu::this_file;
    use crate::src_gen::code_gen::CodeGen;

    #[test]
    fn test_parse_mudul() {
        let r = _test_mudul();
        match r {
            Ok(_) => {}
            Err(e) => match e.ec() {
                EC::MLParseError => {
                    println!("{}", e);
                }
                _ => {}
            },
        }
    }

    fn _test_mudul() -> RS<()> {
        for text in [
            include_str!("ddl_item.sql"),
            include_str!("ddl_warehouse.sql"),
        ] {
            let result = CodeGen::generate_entity_code_from_ddl_sql(text,  "Rust", true)?;
            for (name, src) in result.source_code {
                let fmt = rust_format::RustFmt::new();
                let r = fmt.format_str(&src);
                if r.is_err() {
                    println!("name: {}, source code : {}\n", name, src);
                    let path = PathBuf::from(this_file!())
                        .parent().unwrap()
                        .join("artifact");
                    if !path.exists() {
                        fs::create_dir_all(&path).unwrap()
                    }
                    let path = path.join(format!("{}.rs", name));
                    fs::write(path, src).unwrap();
                }
            }
        }
        Ok(())
    }
}
