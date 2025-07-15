use clap::Parser;
use mudu::common::error::ER;
use mudu::common::result::RS;
use mudu_gen::code_gen::ddl_parser::DDLParser;
use mudu_gen::code_gen::src_gen::{Language, SrcGen};
use std::fs;
use std::fs::read_to_string;
use std::path::Path;

/// 简单的文件复制程序
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// input file path
    #[arg(short, long)]
    in_path: String,

    /// output directory path
    #[arg(short, long)]
    out_path: String,

    /// output language
    #[arg(short, long)]
    lang:Language
}

// read from DDL SQL file, and generate source code
fn gen_for_ddl_sql<P:AsRef<Path>>(
    input_ddl_path:P,
    output_dir_path:P,
    lang:Language
) -> RS<()> {
    let out_path_buf = output_dir_path.as_ref().to_path_buf();
    let sql_text = read_to_string(input_ddl_path)
        .map_err(|e| {
        ER::IOError(format!("open DDL SQL file error {:?}", e))
    })?;
    let ml_parser = DDLParser::new();
    let vec_table_def = ml_parser.parse(&sql_text)?;
    let gen = SrcGen::new();
    for table_def in vec_table_def.iter() {
        let src = gen.gen(lang, &table_def)?;
        let out_src_path = out_path_buf.join(
            format!("{}.{}", table_def.table_name(), lang.lang_suffix()));
        fs::write(&out_src_path, src).map_err(|e|{
            ER::IOError(format!("write generated source code error {:?}", e))
        })?;
        println!("output source code {} for table {}", out_src_path.to_str().unwrap(), table_def.table_name());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    gen_for_ddl_sql(&args.in_path, &args.out_path, args.lang)?;
    Ok(())
}
