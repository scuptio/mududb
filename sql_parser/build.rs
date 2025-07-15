use crate::ts_const_gen::from_gram::gen_rs;
use project_root::get_project_root;
pub mod ts_const_gen;


fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = get_project_root().unwrap();
    println!("path: {:?}", path);
    let path = if path.file_name().unwrap().to_str().unwrap() == "mududb" {
        path
    } else {
        path.join("mududb")
    };
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
