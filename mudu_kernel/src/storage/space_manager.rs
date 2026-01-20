use crate::storage::storage_cfg::StorageCfg;
use crate::storage::table_space::TableSpace;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use scc::HashMap;


pub struct SpaceManager {
    tablespaces: HashMap<u64, TableSpace>,
    base_path: String,
}


impl SpaceManager {
    pub fn new(cfg: &StorageCfg) -> RS<Self> {
        std::fs::create_dir_all(&cfg.path)
            .map_err(|e| { m_error!(EC::IOErr, "create dir error", e) })?;
        Ok(Self {
            tablespaces: HashMap::new(),
            base_path: cfg.path.clone(),
        })
    }


    /// Get the new tablespace
    pub fn get_tablespace(&self, id: u64) -> RS<TableSpace> {
        let opt = self.tablespaces.get_sync(&id);
        match opt {
            Some(tablespace) => {
                Ok(tablespace.clone())
            }
            None => {
                let table_space = TableSpace::new(id, self.base_path.clone())?;
                self.tablespaces.insert_sync(id, table_space.clone());
                Ok(table_space)
            }
        }
    }
}