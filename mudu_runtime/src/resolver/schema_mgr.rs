use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_gen::code_gen::ddl_parser::DDLParser;
use mudu_gen::code_gen::table_def::TableDef;
use std::collections::HashMap;
use std::fs;
use std::fs::read_to_string;
use std::sync::{Arc, Mutex};

const DDL_SQL_EXTENSION: &str = "sql";
#[derive(Clone)]
pub struct SchemaMgr {
    map: Arc<Mutex<HashMap<String, TableDef>>>,
}

impl SchemaMgr {
    pub fn load_from_ddl_path(ddl_path: &String) -> RS<SchemaMgr> {
        let parser = DDLParser::new();
        let schema_mgr = SchemaMgr::new_empty();
        for entry in fs::read_dir(ddl_path)
            .map_err(|e| {
                m_error!(
                    EC::MuduError,
                    format!("read DDL SQL directory {:?} error", ddl_path),
                    e
                )
            })?
        {
            let entry = entry
                .map_err(|e| {
                    m_error!(EC::MuduError, "entry  error", e)
                })?;
            let path = entry.path();

            // check if this is a file
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext.to_ascii_lowercase() == DDL_SQL_EXTENSION {
                        let r = read_to_string(path);
                        let str = match r {
                            Ok(str) => { str }
                            Err(e) => {
                                return Err(
                                    m_error!(
                                        EC::IOErr,
                                        format!("read ddl path {} failed", ddl_path),
                                        e)
                                );
                            }
                        };
                        let table_def_list = parser.parse(&str)?;
                        for table_def in table_def_list {
                            schema_mgr.insert(table_def.table_name().clone(), table_def)?;
                        }
                    }
                }
            }
        }

        Ok(schema_mgr)
    }


    pub fn new_empty() -> Self {
        Self {
            map: Arc::new(Mutex::new(HashMap::new()))
        }
    }

    pub fn insert(&self, key: String, table_def: TableDef) -> RS<bool> {
        let mut g = self.map.lock().unwrap();
        if !g.contains_key(&key) {
            g.insert(key, table_def);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn get(&self, key: &String) -> RS<Option<TableDef>> {
        let g = self.map.lock().unwrap();
        let opt = g.get(key);
        if let Some(def) = opt {
            Ok(Some((*def).clone()))
        } else {
            Ok(None)
        }
    }
}