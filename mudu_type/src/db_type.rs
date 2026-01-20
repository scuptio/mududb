use std::path::Path;
use std::sync::Arc;
use mudu::common::result::RS;
use crate::dat_type::DatType;

pub trait DBType: Send + Sync {
    fn get(&self, name:&String) -> Option<DatType> ;
    fn insert(&self, name: String, dat_type: DatType) -> Result<(), (String, DatType)>;
}


pub fn load_db_types<P: AsRef<Path>>(_: P) -> RS<Arc<dyn DBType>> {
    let db_type = DBTypeImpl::load_types();
    Ok(Arc::new(db_type))
}

pub fn db_type_mgr() -> RS<Arc<dyn DBType>> {
    let db_type = DBTypeImpl::load_types();
    Ok(Arc::new(db_type))
}

struct DBTypeImpl {
    map:scc::HashMap<String, DatType>,
}


impl DBTypeImpl {
    pub fn load_types() -> Self {
        Self {
            map: Default::default()
        }
    }


    fn get_ty(&self, name:&String) -> Option<DatType> {
        let opt = self.map.get_sync(name);
        opt.map(|dat_type| dat_type.clone())
    }

    fn insert_ty(&self, name:String, dat_type:DatType) -> Result<(), (String, DatType)> {
        self.map.insert_sync(name.clone(), dat_type)
    }
}

impl DBType for DBTypeImpl {
    fn get(&self, name: &String) -> Option<DatType> {
        self.get_ty(name)
    }

    fn insert(&self, name: String, dat_type: DatType) -> Result<(), (String, DatType)> {
        self.insert_ty(name, dat_type)
    }
}
