use crate::ts_const_gen::from_gram::gen_rs;
use mudu_build_common::repo_root;
pub mod ts_const_gen;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = repo_root()?;
    let grammar_path = repo_root
        .join("tree-sitter-sql")
        .join("src")
        .join("grammar.json");
    let output_path = repo_root.join("sql_parser").join("src").join("ts_const");
    println!("output path: {:?}", output_path);
    gen_rs(output_path, grammar_path)?;
    Ok(())
}
