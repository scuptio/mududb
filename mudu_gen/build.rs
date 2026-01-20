use crate::ts_const_gen::from_gram::gen_rs;
use std::path::PathBuf;
pub mod ts_const_gen;

fn main() -> Result<(), Box<dyn std::error::Error>>  {
    let path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let path = PathBuf::from(&path).parent().unwrap().to_path_buf();
    let mut grammar_path = path.clone();
    let mut output_path = path.clone();
    let mut md5_path = path.clone();

    grammar_path.push("tree-sitter-wit");
    grammar_path.push("src");
    grammar_path.push("grammar.json");

    output_path.push("mudu_gen");
    output_path.push("src");
    output_path.push("ts_const");


    md5_path.push("mudu_gen");
    md5_path.push("ts_const_gen");
    md5_path.push("grammar.md5.txt");

    gen_rs(output_path, grammar_path, md5_path, tree_sitter_wit::LANGUAGE.into());
    Ok(())
}
