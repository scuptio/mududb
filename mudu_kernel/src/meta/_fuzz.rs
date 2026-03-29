use arbitrary::Unstructured;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use mudu::common::result::RS;

use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;

pub fn fuzz_printable(schema_path: String, output_path: String, u: &mut Unstructured) -> RS<()> {
    if !fs::exists(output_path.clone()).unwrap() {
        fs::create_dir_all(output_path.clone()).unwrap();
    }
    let json = fs::read_to_string(&schema_path)
        .expect(format!("failed to read schema file {}", schema_path).as_str());
    let schema = serde_json::from_str::<SchemaTable>(&json).unwrap();
    let table_name = schema.table_name().clone();

    let mut db_path = PathBuf::from(output_path);
    db_path.push("kv.db");
    let db_path = db_path.as_path().to_str().unwrap().to_string();
    let mut map = HashMap::new();
    let _r = fuzz_data_for_schema(&schema, u, &mut map);
    write_map_to_db(db_path.clone(), table_name, map)?;
    Ok(())
}

pub fn write_data_to_csv(schema_path: String, output_path: String) -> RS<()> {
    let json = fs::read_to_string(&schema_path)
        .expect(format!("failed to read schema file {}", schema_path).as_str());
    let schema = serde_json::from_str::<SchemaTable>(&json).unwrap();
    let table_name = schema.table_name().clone();
    let mut db_path = PathBuf::from(output_path.clone());
    db_path.push("kv.db");
    let db_path = db_path.as_path().to_str().unwrap().to_string();
    let map = read_map_from_db(db_path, table_name)?;
    let output_csv_path = PathBuf::from(output_path.clone());
    let output_csv_path = output_csv_path.to_str().unwrap().to_string();
    write_map_to_csv(output_csv_path, &map)?;
    Ok(())
}

fn write_map_to_csv(output_csv_path: String, map: &HashMap<Vec<String>, Vec<String>>) -> RS<()> {
    let path = PathBuf::from(output_csv_path.clone());
    let parent = path.parent().unwrap();
    if !fs::exists(parent).unwrap() {
        fs::create_dir_all(parent).unwrap();
    }

    let mut file = BufWriter::new(
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(output_csv_path)
            .unwrap(),
    );

    for (k, v) in map.iter() {
        let mut tuple = k.clone();
        tuple.extend(v.clone());
        let s = format_comma_line(&tuple);
        file.write_fmt(format_args!("{}\n", s)).unwrap();
    }
    Ok(())
}

pub fn format_comma_line(vec: &Vec<String>) -> String {
    let mut s_ret = "".to_string();
    for (i, s) in vec.into_iter().enumerate() {
        if i != 0 {
            s_ret.push_str(", ");
        }
        s_ret.push_str(&s);
    }
    s_ret
}

pub fn fuzz_data_for_schema<'a>(
    schema: &SchemaTable,
    u: &mut Unstructured<'a>,
    key_value_map: &mut HashMap<Vec<String>, Vec<String>>,
) -> arbitrary::Result<()> {
    let map = key_value_map;
    loop {
        if u.is_empty() {
            return Ok(());
        }
        fuzz_row_for_schema(schema, u, map)?;
    }
}

fn fuzz_row_for_schema<'a>(
    schema: &SchemaTable,
    u: &mut Unstructured<'a>,
    key_value_map: &mut HashMap<Vec<String>, Vec<String>>,
) -> arbitrary::Result<()> {
    if u.len() == 0 {
        return Ok(());
    }
    let key = loop {
        let mut key = Vec::with_capacity(schema.key_columns().len());
        if u.len() == 0 {
            return Ok(());
        }
        for c in schema.key_columns() {
            let s = arb_string(c, u)?;
            key.push(s);
        }
        if !key_value_map.contains_key(&key) {
            break key;
        }
    };
    let mut value = Vec::with_capacity(schema.value_columns().len());
    for c in schema.value_columns() {
        let s = arb_string(c, u)?;
        value.push(s);
    }
    key_value_map.insert(key, value);
    Ok(())
}

fn arb_string<'a>(c: &SchemaColumn, u: &mut Unstructured<'a>) -> arbitrary::Result<String> {
    let dt = c.type_id();
    let f = dt.fn_arb_printable();
    let dat_type = c.type_param().to_dat_type().unwrap();
    let s = f(u, &dat_type)?;
    Ok(s)
}

fn write_map_to_db(
    path: String,
    table_name: String,
    map: HashMap<Vec<String>, Vec<String>>,
) -> RS<()> {
    let mut db = FuzzDb::load(&path)?;
    let table = db.tables.entry(table_name).or_default();
    for (k, v) in map {
        if !table.iter().any(|row| row.key_items == k) {
            table.push(FuzzRow {
                key_items: k,
                value_items: v,
            });
        }
    }
    db.save(&path)
}

fn read_map_from_db(path: String, table_name: String) -> RS<HashMap<Vec<String>, Vec<String>>> {
    let db = FuzzDb::load(&path)?;
    let mut map = HashMap::new();
    if let Some(rows) = db.tables.get(&table_name) {
        for row in rows {
            map.insert(row.key_items.clone(), row.value_items.clone());
        }
    }
    Ok(map)
}

fn to_json_string(vec: &Vec<String>) -> String {
    serde_json::to_string_pretty(vec).unwrap()
}

fn from_json_string(json: &String) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(json).unwrap()
}

#[derive(Default, Serialize, Deserialize)]
struct FuzzDb {
    tables: HashMap<String, Vec<FuzzRow>>,
}

impl FuzzDb {
    fn load(path: &str) -> RS<Self> {
        if !PathBuf::from(path).exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path).unwrap();
        Ok(serde_json::from_str(&text).unwrap())
    }

    fn save(&self, path: &str) -> RS<()> {
        let parent = PathBuf::from(path).parent().map(|p| p.to_path_buf());
        if let Some(parent) = parent {
            if !fs::exists(&parent).unwrap() {
                fs::create_dir_all(parent).unwrap();
            }
        }
        let text = serde_json::to_string_pretty(self).unwrap();
        fs::write(path, text).unwrap();
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct FuzzRow {
    key_items: Vec<String>,
    value_items: Vec<String>,
}
