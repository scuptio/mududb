#[cfg(test)]
mod tests {
    use crate::src_gen::ddl_parser::DDLParser;
    use crate::src_gen::src_gen::{Language, SrcGen};
    use mudu::common::result::RS;
    use mudu::error::ec::EC;
    use mudu::m_error;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    #[test]
    fn test_mudul_src_gen() {
        let r = test_src_file_gen();
        if r.is_err() {
            let e = r.as_ref().err().unwrap();
            println!("test error : {}", e);
        }
        assert!(r.is_ok());
    }

    fn test_src_file_gen() -> RS<()> {
        for text in vec![
            include_str!("ddl_item.sql"),
            include_str!("ddl_warehouse.sql"),
            include_str!("wallet_app.sql"),
        ] {
            test_src_gen_gut(text)?;
        }
        Ok(())
    }

    fn test_src_gen_gut(text: &str) -> RS<()> {
        let parser = DDLParser::new();
        let vec = parser.parse(text)?;
        let src_gen = SrcGen::new();
        for table_def in vec.iter() {
            let text_src = src_gen.generate(Language::Rust, table_def)?;
            let file_name = format!("{}.rs", table_def.table_name());
            let path = write_string_to_temp_file(text_src, file_name)?;
            let output = Command::new("rustc")
                .arg("--emit=metadata")
                .arg("--crate-type=lib") // crate-type=lib, no main
                .arg("--edition=2024") //   edition
                .arg(&path)
                .output()
                .map_err(|e| m_error!(EC::IOErr, "build command line", e))?;
            if output.status.success() {
                println!("compile {} OK", path.to_str().unwrap());
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("syntax error:\n{}", stderr);
            }
        }

        let path = project_root::get_project_root()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let path = path.join("example");
        std::env::set_current_dir(&path).unwrap();

        let output = Command::new("cargo")
            .arg("fmt")
            .output()
            .map_err(|e| m_error!(EC::IOErr, "cargo fmt error", e))?;
        if output.status.success() {
            println!("cargo fmt OK");
        }

        let output = Command::new("cargo")
            .arg("build")
            .output()
            .map_err(|e| m_error!(EC::IOErr, "cargo build error", e))?;
        if output.status.success() {
            println!("cargo build OK");
        }
        Ok(())
    }

    fn write_string_to_temp_file(content: String, file_name: String) -> RS<PathBuf> {
        let path = project_root::get_project_root()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let path = path.join("example").join("src").join("rust");

        let file_path = path.join(file_name);
        println!("write to temp file: {:?}", file_path);
        fs::write(&file_path, content)
            .map_err(|e| m_error!(EC::IOErr, "write temp file error", e))?;
        Ok(file_path)
    }
}
