use md5::{Digest, Md5};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::{read_to_string, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use tree_sitter::Language;
use tree_sitter_sql::LANGUAGE;


pub fn gen_rs<O: AsRef<Path>, G: AsRef<Path>>(
    output_path: O,
    grammar_path: G,
) {
    let mut constant = Constant {
        node_name: Default::default(),
        field_name: Default::default(),
        seq_index: Default::default(),
    };
    let grammar_path_str = grammar_path.as_ref().to_str().unwrap().to_string();
    let grammar_str = read_to_string(&grammar_path)
        .expect(&format!("grammar json file path {} cannot be found", grammar_path_str));
    let opt_new_md5 = grammar_file_changed(&grammar_str);
    let new_md5 = match opt_new_md5 {
        None => { return }
        // file does not change
        Some(s) => s
    };
    let json: Value = serde_json::from_str(grammar_str.as_str())
        .expect(&format!("parse json file {} failed", grammar_path_str));
    visit_rule(json, &mut constant);
    output_rust_file(output_path, &constant);
    write_grammar_md5(&new_md5);
}


macro_rules! grammar_md5_file {
    () => {
        "grammar.md5.txt"
    };
}

macro_rules! this_file {
    () => {
        _this_file(file!())
    };
}

fn _this_file(file: &str) -> String {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok().unwrap();
    let manifest_dir_path_buf = PathBuf::from(&manifest_dir);
    let manifest_dir = manifest_dir_path_buf.parent().unwrap().to_str().unwrap().to_string();
    let file_path = PathBuf::from(file);
    let path = PathBuf::from(manifest_dir).join(file_path);
    path.to_str().map(|s| s.to_string()).unwrap_or(String::new())
}

fn grammar_file_changed(s: &String) -> Option<String> {
    let mut hasher = Md5::new();
    hasher.update(s);
    let md5_hash = hasher.finalize();
    let mut buf = [0u8; 256];
    let s = base16ct::lower::encode_str(&md5_hash, &mut buf).unwrap();
    let s1 = include_str!(grammar_md5_file!());
    if s == s1 {
        None
    } else {
        Some(s.to_string())
    }
}

macro_rules! write_content {
    ($filename:expr, $content:expr) => {
        {
            // Get the directory of the current source file
            let this_file = this_file!();
            let current_file = Path::new(&this_file);
            let dir = current_file.parent()
                .expect("Failed to get parent directory of current file");
            
            // Create the full path
            let file_path = dir.join($filename);
            
            // Write the content to the file
            let mut file = File::create(&file_path)
                .expect(&format!("Failed to create file: {:?}", file_path));
            file.write_all($content.as_bytes())
                .expect(&format!("Failed to write to file: {:?}", file_path));
            
            println!("Successfully wrote to {:?}", file_path);
        }
    };
}

fn write_grammar_md5(md5: &String) {
    write_content!(grammar_md5_file!(), md5)
}


const COMMENTS: &'static str = include_str!("comments.txt");

const RULES: &'static str = "rules";

const TYPE: &'static str = "type";

const REPEAT: &'static str = "REPEAT";
const REPEAT1: &'static str = "REPEAT1";
const SEQ: &'static str = "SEQ";
const CHOICE: &'static str = "CHOICE";
const FIELD: &'static str = "FIELD";
const PREC: &'static str = "PREC";
const PREC_LEFT: &'static str = "PREC_LEFT";
const PREC_RIGHT: &'static str = "PREC_RIGHT";
const MEMBERS: &'static str = "members";
const CONTENT: &'static str = "content";
const NAME: &'static str = "name";

struct Constant {
    node_name: HashSet<String>,
    field_name: HashSet<String>,
    seq_index: HashMap<String, Vec<usize>>,
}


fn language() -> Language {
    LANGUAGE.clone().into()
}


fn language_name() -> &'static str {
    language().name().unwrap()
}

fn format_name(names: &Vec<String>) -> String {
    let mut name_ret = String::new();
    for (i, name) in names.iter().enumerate() {
        if i != names.len() - 1 {
            let f20char = if name.len() > 20 { &name[0..20] } else { name };
            name_ret.push_str(f20char);
            name_ret.push_str("_");
        } else {
            name_ret.push_str(name);
        }
    }
    name_ret
}

fn visit_a_rule(rule_content: &Value, names: &mut Vec<String>, constant: &mut Constant) {
    let map = rule_content.as_object().expect("as object");
    let value_type = map.get(TYPE).expect("must have type");
    let type_name = value_type.as_str().expect("type must be string");
    let mut node_name = type_name.to_string();
    names.push(node_name.clone());
    match type_name {
        SEQ => {
            let value_members = map.get(MEMBERS).expect("SEQ type must have members");
            let members = value_members.as_array().expect("members must be array");
            for (i, m) in members.iter().enumerate() {
                let value_member = m.as_object().expect("member must be object");
                let name = if let Some(v_name) = value_member.get(language_name()) {
                    let name = v_name.as_str().expect("name must be string").to_string();
                    name
                } else if let Some(v_type) = value_member.get(TYPE) {
                    v_type.as_str().expect("type must be string").to_string()
                } else {
                    panic!("member must have a type");
                };
                names.push(name);
                let formated_name = format_name(&names);
                names.pop();
                let opt_value = constant.seq_index.get_mut(&formated_name);
                match opt_value {
                    Some(vec) => {
                        // existing such name
                        if !vec.contains(&i) {
                            // ignore if existing this index
                            vec.push(i);
                        }
                    }
                    None => {
                        constant.seq_index.insert(formated_name, vec![i]);
                    }
                }
                visit_a_rule(m, names, constant);
            }
        }
        CHOICE => {
            let value_members = map.get(MEMBERS).expect("CHOICE type must have members");
            let members = value_members.as_array().expect("members must be array");
            for m in members.iter() {
                visit_a_rule(m, names, constant);
            }
        }
        FIELD => {
            let value_content = map.get(CONTENT).expect("FIELD type must have content");
            let value_name = map.get(NAME).expect("field must have name");
            let field_name = value_name
                .as_str()
                .expect("name must be string")
                .to_string();
            constant.field_name.insert(field_name);
            visit_a_rule(value_content, names, constant);
        }
        REPEAT | REPEAT1 | PREC | PREC_LEFT | PREC_RIGHT => {
            let value_content = map.get(CONTENT).expect("REPEAT type must have content");
            visit_a_rule(value_content, names, constant);
        }
        _ => {
            let opt = map.get(language_name());
            if let Some(name) = opt {
                node_name = name.as_str().expect("name must be string").to_string();
                names.pop();
                names.push(node_name);
            }
        }
    }
    names.pop();
}

fn visit_rule(json: Value, constant: &mut Constant) {
    let map = json.as_object().expect("json must be object");
    let value_rules = map.get(RULES).expect("rules missing");
    let map_rules = value_rules
        .as_object()
        .expect("rules value as object failed");
    for (key, value) in map_rules.iter() {
        let mut names = vec![key.clone()];
        constant.node_name.insert(key.clone());
        visit_a_rule(value, &mut names, constant);
    }
}


fn output_rust_file<P: AsRef<Path>>(path: P, constant: &Constant) {
    let mut node_kind_id: Vec<(String, u16)> = constant
        .node_name
        .iter()
        .map(|k| {
            let id = language().id_for_node_kind(&k, true);
            (k.clone(), id)
        })
        .collect();
    node_kind_id.sort_by(|(_, id1), (_, id2)| id1.cmp(&id2));

    let mut field_name: Vec<String> = constant.field_name.iter().cloned().collect();
    field_name.sort();

    let mut seq_index: Vec<(String, Vec<usize>)> = constant
        .seq_index
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    seq_index.sort_by(|(n1, _), (n2, _)| n1.cmp(n2));

    let path_ref = path.as_ref();
    let path_buf = PathBuf::from(path_ref);
    let mut path_field_names = path_buf.clone();
    let mut path_field_ids = path_buf.clone();
    let mut path_kind_name_ids = path_buf.clone();
    let mut path_kind_names = path_buf.clone();
    let mut path_seq_index = path_buf.clone();

    path_field_names.push("ts_field_name.rs");
    path_field_ids.push("ts_field_id.rs");
    path_kind_name_ids.push("ts_kind_id.rs");
    path_kind_names.push("ts_kind_name.rs");
    path_seq_index.push("ts_seq_index.rs");

    let mut file_kind_name_ids = File::create(path_kind_name_ids).unwrap();
    let mut file_kind_names = File::create(path_kind_names).unwrap();
    let mut file_field_names = File::create(path_field_names).unwrap();
    let mut file_field_ids = File::create(path_field_ids).unwrap();
    let mut file_seq_index = File::create(path_seq_index).unwrap();

    file_kind_name_ids
        .write_fmt(format_args!("{}", COMMENTS))
        .unwrap();
    file_kind_name_ids
        .write_fmt(format_args!("// kind id of Node\n\n"))
        .unwrap();

    file_kind_names
        .write_fmt(format_args!("{}", COMMENTS))
        .unwrap();
    file_kind_names
        .write_fmt(format_args!("// kind name of Node\n\n"))
        .unwrap();
    for (name, id) in node_kind_id {
        let mut var_name = name.clone();
        let mut name_str = name.clone();

        var_name.make_ascii_uppercase();
        file_kind_name_ids
            .write_fmt(format_args!("pub const {} : u16 = {};\n", var_name, id))
            .unwrap();

        name_str.make_ascii_lowercase();
        file_kind_names
            .write_fmt(format_args!("pub const S_{} : &str = \"{}\";\n", var_name, name_str))
            .unwrap();
    }

    file_field_names
        .write_fmt(format_args!("{}", COMMENTS))
        .unwrap();
    file_field_names
        .write_fmt(format_args!("// field name\n\n"))
        .unwrap();
    file_field_ids
        .write_fmt(format_args!("{}", COMMENTS))
        .unwrap();
    file_field_ids
        .write_fmt(format_args!("// field id\n\n"))
        .unwrap();
    for field_name in field_name {
        let mut upper_case_name = field_name.clone();
        upper_case_name.make_ascii_uppercase();
        file_field_names
            .write_fmt(format_args!(
                "pub const {} : &'static str = \"{}\";\n",
                upper_case_name, field_name
            ))
            .unwrap();

        let opt_id = language().field_id_for_name(&field_name);

        if let Some(id) = opt_id {
            file_field_ids
                .write_fmt(format_args!(
                    "pub const FI_{} : u16 = {};\n",
                    upper_case_name, id
                ))
                .unwrap();
        }
    }

    file_seq_index
        .write_fmt(format_args!("{}", COMMENTS))
        .unwrap();
    file_seq_index
        .write_fmt(format_args!("// sequence index in array of SEQ type\n\n"))
        .unwrap();
    for (name, index) in seq_index {
        let mut name = name;
        name.make_ascii_uppercase();
        if index.len() == 1 {
            let i = index[0];
            file_seq_index
                .write_fmt(format_args!("pub const {} : usize = {};\n", name, i))
                .unwrap();
        } else if index.len() > 1 {
            for i in index {
                file_seq_index
                    .write_fmt(format_args!("pub const {}_{} : usize = {};\n", name, i, i))
                    .unwrap();
            }
        }
    }
}
