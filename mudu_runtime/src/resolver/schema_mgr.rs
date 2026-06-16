use lazy_static::lazy_static;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use sql_parser::parser::ddl_parser::DDLParser;

use mudu_binding::record::record_def::RecordDef;
use scc::HashMap as SCCHashMap;
use std::collections::HashMap;
use mudu_sys::fs;
use std::sync::Arc;

const DDL_SQL_EXTENSION: &str = "sql";
#[derive(Clone)]
pub struct SchemaMgr {
    tables: Arc<HashMap<String, RecordDef>>,
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
    pub fn from_sql_text(sql_text: &str) -> RS<SchemaMgr> {
        let parser = DDLParser::new();
        let tables = load_table_map_from_sql_text(sql_text, &parser)?;
        Ok(Self {
            tables: Arc::new(tables),
        })
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
        let mut tables = HashMap::new();
        for entry in fs::sync::sync_read_dir_entries(ddl_path).map_err(|e| {
            m_error!(
                EC::MuduError,
                format!("read DDL SQL directory {:?} error", ddl_path),
                e
            )
        })? {
            let path = entry.path();

            // check if this is a file
            if path.is_file()
                && let Some(ext) = path.extension()
                    && ext.to_ascii_lowercase() == DDL_SQL_EXTENSION {
                        let str = fs::sync::sync_read_to_string(&path).map_err(|e| {
                            m_error!(
                                EC::IOErr,
                                format!("read ddl path {} failed", ddl_path),
                                e
                            )
                        })?;
                        tables.extend(load_table_map_from_sql_text(&str, &parser)?);
                    }
        }

        Ok(Self {
            tables: Arc::new(tables),
        })
    }

    pub fn get(&self, key: &String) -> RS<Option<RecordDef>> {
        Ok(self.tables.get(key).cloned())
    }

    pub fn table_names(&self) -> Vec<String> {
        self.tables.keys().cloned().collect()
    }
}

fn load_table_map_from_sql_text(
    sql_text: &str,
    parser: &DDLParser,
) -> RS<HashMap<String, RecordDef>> {
    let table_def_list = parser.parse(sql_text)?;
    let mut tables = HashMap::with_capacity(table_def_list.len());
    for table_def in table_def_list {
        tables.insert(table_def.table_name().clone(), table_def);
    }
    Ok(tables)
}
