use mudu::this_file;
use std::path::PathBuf;

pub fn wasm_mod_path() -> String {
    let wasm_path = PathBuf::from(this_file!())
        .parent()
        .unwrap()
        .to_path_buf()
        .parent()
        .unwrap()
        .to_path_buf()
        .parent()
        .unwrap()
        .to_path_buf()
        .parent()
        .unwrap()
        .to_path_buf()
        .join("mudu_wasm".to_string())
        .join("wasm_module".to_string());
    wasm_path.to_str().unwrap().to_string()
}
