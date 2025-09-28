use crate::contract::meta_mgr::MetaMgr;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::common::result_of::rs_io;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use std::collections::HashMap;
use std::fs;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

pub struct MetaMgrImpl {
    path: String,
    id2table: scc::HashMap<OID, TableInfo>,
    name2id: scc::HashMap<String, OID>,
    table: scc::HashMap<String, TableInfo>,
}

impl MetaMgrImpl {
    pub fn new<P: AsRef<Path>>(path: P) -> RS<Self> {
        let mut hash_table = HashMap::new();
        let path = PathBuf::from(path.as_ref());
        if fs::metadata(path.clone()).is_err() {
            fs::create_dir(path.clone()).map_err(|e| { m_error!(ER::IOErr,"", e) })?;
        }

        for entry in rs_io(fs::read_dir(path.clone()))? {
            let entry = rs_io(entry)?;
            let path = entry.path();

            let metadata = rs_io(fs::metadata(&path))?;
            if metadata.is_file() {
                let schema = Self::read_schema_from_file(&path.to_str().unwrap().to_string())?;
                hash_table.insert(schema.table_name().to_string(), TableInfo::new(schema));
            }
        }

        Ok(Self {
            path: path.to_str().unwrap().to_string(),
            id2table: Default::default(),
            name2id: Default::default(),
            table: Default::default(),
        })
    }

    pub fn get_table_by_id(&self, oid: OID) -> Option<TableInfo> {
        let opt = self.id2table.get(&oid);
        opt.map(|e| e.get().clone())
    }

    pub fn get_table_by_name(&self, name: &String) -> RS<Option<Arc<TableDesc>>> {
        let opt = self.table.get(name);
        let table_desc = match opt {
            None => return Ok(None),
            Some(t) => t.get().table_desc()?,
        };
        Ok(Some(table_desc))
    }

    pub fn _create_table(&self, schema: &SchemaTable) -> RS<()> {
        if !self.table.contains(schema.table_name()) {
            let table_name = schema.table_name().clone();
            let mut pb = PathBuf::from(self.path.clone());
            pb.push(format!("{}.json", schema.table_name().clone()));
            let r = Self::write_schema_to_file(&pb.to_str().unwrap().to_string(), &schema);
            match r {
                Ok(_) => {}
                Err(e) => {
                    info!("{:?}", e)
                }
            }
            let table_id = schema.id();
            let table = TableInfo::new(schema.clone());
            let _ = self.table.insert(table_name.clone(), table.clone());
            let _ = self.id2table.insert(table_id, table);
            let _ = self.name2id.insert(table_name, table_id);
        } else {
            return Err(m_error!(ER::ExistingSuchElement, ""));
        }
        Ok(())
    }

    fn read_schema_from_file(path: &String) -> RS<SchemaTable> {
        let r_open = File::open(path);
        let file = rs_io(r_open)?;
        let r_from_reader = serde_json::from_reader::<_, SchemaTable>(file);
        let schema = match r_from_reader {
            Ok(e) => e,
            Err(e) => {
                return Err(m_error!(ER::DecodeErr, "read schema error", e));
            }
        };
        Ok(schema)
    }

    fn write_schema_to_file(path: &String, schema: &SchemaTable) -> RS<()> {
        let r_open = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path);
        let file = rs_io(r_open)?;
        let r = serde_json::to_writer_pretty(file, schema);
        match r {
            Ok(_) => Ok(()),
            Err(e) => Err(m_error!(ER::EncodeErr, "write schema error", e)),
        }
    }
}

#[async_trait]
impl MetaMgr for MetaMgrImpl {
    async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
        let opt = self.get_table_by_id(oid);
        match opt {
            Some(t) => t.table_desc(),
            None => Err(m_error!(ER::NoSuchElement, format!("no such table {}", oid))),
        }
    }

    async fn get_table_by_name(&self, name: &String) -> RS<Option<Arc<TableDesc>>> {
        self.get_table_by_name(name)
    }

    async fn create_table(&self, schema: &SchemaTable) -> RS<()> {
        self._create_table(schema)
    }

    async fn drop_table(&self, _oid: OID) -> RS<()> {
        todo!()
    }
}

unsafe impl Sync for MetaMgrImpl {}

unsafe impl Send for MetaMgrImpl {}
