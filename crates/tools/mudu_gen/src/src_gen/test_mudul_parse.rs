#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::src_gen::code_gen::CodeGen;
    use mudu::common::result::RS;

    // Miri cannot execute FFI calls into the tree-sitter C parser, so skip this
    // test under Miri. SQL/DDL parsing is still exercised by normal `cargo test`.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_parse_mudul() {
        // Hard failure on any generation or syntax error: the generated
        // Rust must always parse.
        _test_mudul().unwrap();
    }

    fn _test_mudul() -> RS<()> {
        for text in [
            include_str!("ddl_item.sql"),
            include_str!("ddl_warehouse.sql"),
        ] {
            let result = CodeGen::generate_object_code_from_ddl_sql(text, "Rust", true)?;
            for (_name, src) in result.source_code {
                syn::parse_file(&src)
                    .map(|syntax| prettyplease::unparse(&syntax))
                    .unwrap();
            }
        }
        Ok(())
    }
}
