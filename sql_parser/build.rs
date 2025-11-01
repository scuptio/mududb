use crate::ts_const_gen::from_gram::gen_rs;
use std::path::PathBuf;
pub mod ts_const_gen;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let path = PathBuf::from(&path).parent().unwrap().to_path_buf();
    let mut grammar_path = path.clone();
    let mut output_path = path.clone();

    grammar_path.push("tree-sitter-sql");
    grammar_path.push("src");
    grammar_path.push("grammar.json");

    output_path.push("sql_parser");
    output_path.push("src");
    output_path.push("ts_const");
    println!("output path: {:?}", output_path);
    gen_rs(output_path, grammar_path);
    Ok(())
}
