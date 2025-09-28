use crate::meta::_fuzz::{fuzz_printable, write_data_to_csv};
use arbitrary::Unstructured;
use std::path::PathBuf;

fn root_path() -> String {
    let _p = PathBuf::from(file!());
    let path = _p
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    path
}

fn input_output_path(name: &str) -> (String, String) {
    let path = root_path();
    let input_json = PathBuf::from(path.clone());
    let output_folder = PathBuf::from(path.clone());
    let input_json = input_json.join(format!("data/meta/{}.json", name));
    let output_folder = output_folder.join("data/meta/");
    let input_json = input_json.as_os_str().to_str().unwrap().to_string();
    let output_folder = output_folder.as_os_str().to_str().unwrap().to_string();
    (input_json, output_folder)
}

fn fuzz_gen_csv(u: &[u8], name: &str) {
    let (input_file, output_folder) = input_output_path(name);
    let mut u = Unstructured::new(u);
    fuzz_printable(input_file, output_folder, &mut u).unwrap();
}

pub fn _gen_order_csv(u: &[u8]) {
    fuzz_gen_csv(u, "oorder");
}

fn _test_gen_csv(name: &str) {
    let (input_file, output_folder) = input_output_path(name);
    write_data_to_csv(input_file, output_folder).unwrap();
}
