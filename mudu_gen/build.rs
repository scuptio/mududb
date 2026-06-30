use mudu_build_common::repo_root;
use mudu_build_common::ts_const_generate;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = repo_root()?;

    let grammar_path = repo_root
        .join("tree-sitter-wit")
        .join("src")
        .join("grammar.json");
    let output_path = repo_root.join("mudu_gen").join("src").join("ts_const");
    let md5_path = repo_root
        .join("mudu_gen")
        .join("ts_const_gen")
        .join("grammar.md5.txt");

    ts_const_generate(
        output_path,
        grammar_path,
        Some(md5_path),
        tree_sitter_wit::LANGUAGE.into(),
    )?;
    Ok(())
}
