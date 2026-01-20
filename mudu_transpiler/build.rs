use anyhow::Result;
use cargo_metadata::MetadataCommand;
use mudu::this_file;
use mudu_utils::ts_gram::gen_rs;
use std::path::{Path, PathBuf};
use tree_sitter::Language;

fn main() -> Result<()> {
    let metadata = MetadataCommand::new()
        .exec()
        .expect("failed to get metadata");
    let gram_list: Vec<(&str, &str, Language)> = vec![(
        "tree-sitter-rust",
        "rust",
        tree_sitter_rust::LANGUAGE.into(),
    )];
    for (dep_target_name, lang_name, lang) in gram_list.iter() {
        // search package
        for package in &metadata.packages {
            if package.name == dep_target_name {
                let path = PathBuf::from(&package.manifest_path)
                    .to_path_buf()
                    .parent()
                    .unwrap()
                    .to_path_buf();
                gen_const(&path, lang_name, lang)?;

                break;
            }
        }
    }
    Ok(())
}

fn gen_const<P: AsRef<Path>>(path: P, folder: &str, lang: &Language) -> Result<()> {
    let mut grammar_path = PathBuf::from(path.as_ref());
    grammar_path.push("src");
    grammar_path.push("grammar.json");

    let mut output_path = PathBuf::from(this_file!()).parent().unwrap().to_path_buf();
    output_path.push("src");
    output_path.push(folder);
    output_path.push("ts_const");

    let mut md5_path = PathBuf::from(&output_path);
    md5_path.push("md5");

    gen_rs(output_path, grammar_path, md5_path, lang);
    Ok(())
}
