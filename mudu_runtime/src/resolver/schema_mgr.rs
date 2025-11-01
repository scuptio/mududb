use lazy_static::lazy_static;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_gen::src_gen::ddl_parser::DDLParser;
use mudu_gen::src_gen::table_def::TableDef;
use scc::HashMap as SCCHashMap;
use std::collections::HashMap;
use std::fs;
use std::fs::read_to_string;
use std::sync::{Arc, Mutex};

const DDL_SQL_EXTENSION: &str = "sql";
#[derive(Clone)]
pub struct SchemaMgr {
    map: Arc<Mutex<HashMap<String, TableDef>>>,
}

lazy_static! {
    static ref _MGR: SCCHashMap<String, SchemaMgr> = SCCHashMap::new();
}

fn _mgr_get(app_name: &String) -> Option<SchemaMgr> {
    _MGR.get_sync(app_name).map(|e| e.get().clone())
}

fn _mgr_add(app_name: String, schema_mgr: SchemaMgr) {
    let _ = _MGR.insert_sync(app_name, schema_mgr);
}

fn _mgr_remove(app_name: &String) {
    let _ = _MGR.remove_sync(app_name);
}

impl SchemaMgr {
    pub fn from_sql_text(sql_text: &String) -> RS<SchemaMgr> {
        let schema_mgr = SchemaMgr::new_empty();
        let parser = DDLParser::new();
        schema_mgr.load_from_sql_text(sql_text, &parser)?;
        Ok(schema_mgr)
    }

    pub fn get_mgr(app_name: &String) -> Option<SchemaMgr> {
        _mgr_get(app_name)
    }

    pub fn add_mgr(app_name: String, schema_mgr: SchemaMgr) {
        _mgr_add(app_name, schema_mgr);
    }

    pub fn remove_mgr(app_name: &String) {
        _mgr_remove(app_name);
    }

    pub fn load_from_ddl_path(ddl_path: &String) -> RS<SchemaMgr> {
        let parser = DDLParser::new();
        let schema_mgr = SchemaMgr::new_empty();
        for entry in fs::read_dir(ddl_path).map_err(|e| {
            m_error!(
                EC::MuduError,
                format!("read DDL SQL directory {:?} error", ddl_path),
                e
            )
        })? {
            let entry = entry.map_err(|e| m_error!(EC::MuduError, "entry  error", e))?;
            let path = entry.path();

            // check if this is a file
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext.to_ascii_lowercase() == DDL_SQL_EXTENSION {
                        let r = read_to_string(path);
                        let str = match r {
                            Ok(str) => str,
                            Err(e) => {
                                return Err(m_error!(
                                    EC::IOErr,
                                    format!("read ddl path {} failed", ddl_path),
                                    e
                                ));
                            }
                        };
                        schema_mgr.load_from_sql_text(&str, &parser)?;
                    }
                }
            }
        }

        Ok(schema_mgr)
    }

    pub fn new_empty() -> Self {
        Self {
            map: Arc::new(Mutex::new(HashMap::new())),
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

    fn load_from_sql_text(&self, sql_text: &String, parser: &DDLParser) -> RS<()> {
        let table_def_list = parser.parse(sql_text)?;
        for table_def in table_def_list {
            self.insert(table_def.table_name().clone(), table_def)?;
        }
        Ok(())
    }
}
