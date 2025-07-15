use mudu::common::result::RS;
use mudu_gen::code_gen::table_def::TableDef;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct SchemaMgr {
    map: Arc<Mutex<HashMap<String, TableDef>>>
}

impl SchemaMgr {
    pub fn new() -> Self {
        Self {
            map: Arc::new(Mutex::new(HashMap::new()))
        }
    }

    pub fn insert(& self, key:String, table_def:TableDef) -> RS<bool> {
        let mut g = self.map.lock().unwrap();
        if !g.contains_key(&key) {
            g.insert(key, table_def);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    pub fn get(&self, key:&String) -> RS<Option<TableDef>> {
        let g = self.map.lock().unwrap();
        let opt = g.get(key);
        if let Some(def) = opt {
            Ok(Some((*def).clone()))
        } else {
            Ok(None)
        }
    }
}