#[cfg(test)]
mod tests {
    use crate::code_gen::ddl_parser::DDLParser;
    use crate::code_gen::src_gen::{Language, SrcGen};
    use mudu::common::error::ER;
    use mudu::common::result::RS;

    #[test]
    fn test_parse_mudul() {
        let r = _test_mudul();
        match r {
            Ok(_) => {}
            Err(e) => match e {
                ER::MLParseError(s) => {
                    println!("{}", s);
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
            let parser = DDLParser::new();
            let vec = parser.parse(text)?;
            println!("{:?}", vec);
            let src_gen = SrcGen::new();
            for table_def in vec.iter() {
                let src = src_gen.gen(Language::Rust, table_def)?;
                println!("{}", src);
            }
        }
        Ok(())
    }
}
