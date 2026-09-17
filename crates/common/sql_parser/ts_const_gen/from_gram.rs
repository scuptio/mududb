use mudu_build_common::ts_const_generate;
use std::path::Path;
use tree_sitter_sql::LANGUAGE;

pub fn gen_rs<O: AsRef<Path>, G: AsRef<Path>>(
    output_path: O,
    grammar_path: G,
) -> Result<(), Box<dyn std::error::Error>> {
    ts_const_generate(output_path, grammar_path, None::<&Path>, LANGUAGE.into())?;
    Ok(())
}
