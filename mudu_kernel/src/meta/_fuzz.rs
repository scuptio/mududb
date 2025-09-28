use arbitrary::Unstructured;
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

    let param = c.type_param().to_object();
    let s = f(u, &param)?;
    Ok(s)
}

fn write_map_to_db(
    path: String,
    table_name: String,
    map: HashMap<Vec<String>, Vec<String>>,
) -> RS<()> {
    let mut conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(
        format!(
            "CREATE TABLE IF NOT EXISTS {} \
        (key_items TEXT PRIMARY KEY, value_items TEXT NOT NULL);",
            table_name
        )
            .as_str(),
        (),
    )
        .unwrap();
    let trans = conn.transaction().unwrap();
    for (k, v) in map.iter() {
        let key = to_json_string(k);
        let value = to_json_string(v);
        trans
            .execute(
                format!(
                    "INSERT INTO {} (key_items, value_items) VALUES(?1, ?2)\
            ON CONFLICT(key_items) DO NOTHING;\
            ",
                    table_name
                )
                    .as_str(),
                (key, value),
            )
            .unwrap();
    }
    trans.commit().unwrap();
    Ok(())
}

fn read_map_from_db(path: String, table_name: String) -> RS<HashMap<Vec<String>, Vec<String>>> {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(
        format!(
            "CREATE TABLE IF NOT EXISTS {} \
        (key_items TEXT PRIMARY KEY, value_items TEXT NOT NULL);",
            table_name
        )
            .as_str(),
        (),
    )
        .unwrap();
    let mut stmt = conn
        .prepare(format!("SELECT key_items, value_items FROM {}", table_name).as_str())
        .unwrap();
    let mut map = HashMap::new();
    let iter = stmt
        .query_map([], |row| {
            let k: String = row.get(0).unwrap();
            let v: String = row.get(1).unwrap();
            Ok((k, v))
        })
        .unwrap();
    for r in iter {
        let (k, v) = r.unwrap();
        let key = from_json_string(&k);
        let value = from_json_string(&v);
        map.insert(key, value);
    }
    Ok(map)
}

fn to_json_string(vec: &Vec<String>) -> String {
    serde_json::to_string_pretty(vec).unwrap()
}

fn from_json_string(json: &String) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(json).unwrap()
}
