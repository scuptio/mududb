#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]
use arbitrary::Unstructured;
use mudu_sys::fs::sync::{
    sync_create_dir_all, sync_path_exists, sync_read_to_string, sync_write, SOpenOptions,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use mudu::common::result::RS;

use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;

pub fn fuzz_printable(schema_path: String, output_path: String, u: &mut Unstructured) -> RS<()> {
    if !sync_path_exists(&output_path) {
        sync_create_dir_all(output_path.clone()).unwrap();
    }
    let json = sync_read_to_string(&schema_path)
        .unwrap_or_else(|_| panic!("failed to read schema file {}", schema_path));
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
    let json = sync_read_to_string(&schema_path)
        .unwrap_or_else(|_| panic!("failed to read schema file {}", schema_path));
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
    if !sync_path_exists(parent) {
        sync_create_dir_all(parent).unwrap();
    }

    let mut file = BufWriter::new(
        SOpenOptions::new()
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

pub fn format_comma_line(vec: &[String]) -> String {
    let mut s_ret = "".to_string();
    for (i, s) in vec.iter().enumerate() {
        if i != 0 {
            s_ret.push_str(", ");
        }
        s_ret.push_str(s);
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
    if u.is_empty() {
        return Ok(());
    }
    let key = loop {
        let key_columns = schema.key_columns();
        let mut key = Vec::with_capacity(key_columns.len());
        if u.is_empty() {
            return Ok(());
        }
        for c in key_columns {
            let s = arb_string(c, u)?;
            key.push(s);
        }
        if !key_value_map.contains_key(&key) {
            break key;
        }
    };
    let value_columns = schema.value_columns();
    let mut value = Vec::with_capacity(value_columns.len());
    for c in value_columns {
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

#[derive(Default, Serialize, Deserialize)]
struct FuzzDb {
    tables: HashMap<String, Vec<FuzzRow>>,
}

impl FuzzDb {
    fn load(path: &str) -> RS<Self> {
        if !sync_path_exists(path) {
            return Ok(Self::default());
        }
        let text = sync_read_to_string(path).unwrap();
        Ok(serde_json::from_str(&text).unwrap())
    }

    fn save(&self, path: &str) -> RS<()> {
        let parent = PathBuf::from(path).parent().map(|p| p.to_path_buf());
        if let Some(parent) = parent {
            if !sync_path_exists(&parent) {
                sync_create_dir_all(parent).unwrap();
            }
        }
        let text = serde_json::to_string_pretty(self).unwrap();
        sync_write(path, text).unwrap();
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct FuzzRow {
    key_items: Vec<String>,
    value_items: Vec<String>,
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::*;
    use crate::contract::schema_column::SchemaColumn;
    use crate::contract::schema_table::SchemaTable;
    use mudu_sys::fs::sync::sync_remove_dir_all;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dt_info::DTInfo;
    use std::collections::HashSet;
    use std::path::PathBuf;

    fn test_tmp_dir(test_name: &str) -> PathBuf {
        let base = mudu_sys::env_var::var("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target"));
        base.join("tmp")
            .join("mudu_kernel_fuzz_tests")
            .join(test_name)
    }

    fn i32_column(name: &str) -> SchemaColumn {
        SchemaColumn::new(
            name.to_string(),
            DatTypeID::I32,
            DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
        )
    }

    fn tiny_schema() -> SchemaTable {
        SchemaTable::new(
            "kv".to_string(),
            vec![i32_column("k"), i32_column("v")],
            vec![0],
            vec![1],
        )
    }

    #[test]
    fn format_comma_line_empty() {
        assert_eq!(format_comma_line(&[]), "");
    }

    #[test]
    fn format_comma_line_single() {
        assert_eq!(format_comma_line(&["a".to_string()]), "a");
    }

    #[test]
    fn format_comma_line_multiple() {
        assert_eq!(
            format_comma_line(&["a".to_string(), "b".to_string(), "c".to_string()]),
            "a, b, c"
        );
    }

    #[test]
    fn fuzz_data_for_schema_populates_map_for_tiny_schema() {
        let schema = tiny_schema();
        let bytes: Vec<u8> = (0..512).map(|i| i as u8).collect();
        let mut u = Unstructured::new(&bytes);
        let mut map = HashMap::new();
        fuzz_data_for_schema(&schema, &mut u, &mut map).unwrap();

        assert!(!map.is_empty(), "expected at least one generated row");

        let mut keys = HashSet::new();
        for (k, v) in &map {
            assert_eq!(k.len(), schema.key_columns().len());
            assert_eq!(v.len(), schema.value_columns().len());
            assert!(keys.insert(k.clone()), "expected unique keys");
        }
    }

    #[test]
    fn fuzz_db_save_load_roundtrip() {
        let dir = test_tmp_dir("fuzz_db_roundtrip");
        let _ = sync_remove_dir_all(&dir);
        let path = dir.join("kv.db").to_str().unwrap().to_string();

        let mut db = FuzzDb::default();
        db.tables.insert(
            "kv".to_string(),
            vec![
                FuzzRow {
                    key_items: vec!["1".to_string()],
                    value_items: vec!["one".to_string()],
                },
                FuzzRow {
                    key_items: vec!["2".to_string()],
                    value_items: vec!["two".to_string()],
                },
            ],
        );
        db.save(&path).unwrap();

        let loaded = FuzzDb::load(&path).unwrap();
        assert_eq!(loaded.tables.len(), 1);
        let rows = loaded.tables.get("kv").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].key_items, vec!["1"]);
        assert_eq!(rows[0].value_items, vec!["one"]);
        assert_eq!(rows[1].key_items, vec!["2"]);
        assert_eq!(rows[1].value_items, vec!["two"]);
    }

    #[test]
    fn write_map_to_csv_writes_readable_csv() {
        let dir = test_tmp_dir("write_map_to_csv");
        let _ = sync_remove_dir_all(&dir);
        let path = dir.join("out.csv").to_str().unwrap().to_string();

        let mut map = HashMap::new();
        map.insert(vec!["k1".to_string()], vec!["v1".to_string()]);
        map.insert(vec!["k2".to_string()], vec!["v2".to_string()]);
        write_map_to_csv(path.clone(), &map).unwrap();

        let text = sync_read_to_string(&path).unwrap();
        let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
        lines.sort();
        assert_eq!(lines, vec!["k1, v1".to_string(), "k2, v2".to_string()]);
    }

    #[test]
    fn fuzz_printable_generates_db_from_schema_file() {
        let dir = test_tmp_dir("fuzz_printable");
        let _ = sync_remove_dir_all(&dir);
        sync_create_dir_all(&dir).unwrap();
        let schema_path = dir.join("schema.json").to_str().unwrap().to_string();
        let output_path = dir.join("out").to_str().unwrap().to_string();

        let schema = tiny_schema();
        let schema_json = serde_json::to_string_pretty(&schema).unwrap();
        sync_write(&schema_path, schema_json).unwrap();

        let bytes: Vec<u8> = (0..512).map(|i| i as u8).collect();
        let mut u = Unstructured::new(&bytes);
        fuzz_printable(schema_path, output_path.clone(), &mut u).unwrap();

        let db_path = PathBuf::from(output_path).join("kv.db");
        let map = read_map_from_db(
            db_path.to_str().unwrap().to_string(),
            schema.table_name().clone(),
        )
        .unwrap();
        assert!(!map.is_empty());
        for (k, v) in &map {
            assert_eq!(k.len(), schema.key_columns().len());
            assert_eq!(v.len(), schema.value_columns().len());
        }
    }
}
