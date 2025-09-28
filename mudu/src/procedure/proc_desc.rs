use crate::common::result::RS;
use crate::error::ec::EC;
use crate::m_error;
use crate::tuple::tuple_item_desc::TupleItemDesc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(
    Clone,
    Serialize,
    Deserialize
)]
pub struct ProcDesc {
    proc_name: String,
    module_name: String,
    param_desc: TupleItemDesc,
    return_desc: TupleItemDesc,
}

impl ProcDesc {
    pub fn new(
        proc_name: String,
        module_name: String,
        param_desc: TupleItemDesc,
        return_desc: TupleItemDesc,
    ) -> ProcDesc {
        Self {
            proc_name,
            module_name,
            param_desc,
            return_desc,
        }
    }

    pub fn module_name(&self) -> &String {
        &self.module_name
    }
    pub fn proc_name(&self) -> &String {
        &self.proc_name
    }

    pub fn param_desc(&self) -> &TupleItemDesc {
        &self.param_desc
    }

    pub fn return_desc(&self) -> &TupleItemDesc {
        &self.return_desc
    }

    pub fn to_toml_str(&self) -> String {
        toml::to_string_pretty(&self).unwrap()
    }

    pub fn write_to_file<P: AsRef<Path>>(&self, path: P) -> RS<()> {
        let s = self.to_toml_str();
        fs::write(path, s).map_err(|e| {
            m_error!(EC::IOErr, "write to file error", e)
        })?;
        Ok(())
    }

    pub fn from_path<P: AsRef<Path>>(path: P) -> RS<Self> {
        let s = fs::read_to_string(path)
            .map_err(|e| {
                m_error!(EC::IOErr, "read path error", e)
            })?;
        let ret: Self = toml::from_str::<Self>(&s)
            .map_err(|e| {
                m_error!(EC::DecodeErr,  "decode from toml string error", e)
            })?;
        Ok(ret)
    }
}

#[cfg(test)]
mod test {
    use crate::procedure::proc_desc::ProcDesc;
    use crate::tuple::rs_tuple_datum::RsTupleDatum;
    use std::env::temp_dir;

    #[test]
    fn test_proc_desc() {
        let param_desc = <(i32, i32, i64)>::tuple_desc_static();
        let return_desc = <(i32, String)>::tuple_desc_static();
        let proc_desc = ProcDesc::new(
            "proc".to_string(),
            "mudu_wasm".to_string(),
            param_desc,
            return_desc,
        );
        let path = format!("{}/proc_desc.toml", temp_dir().to_str().unwrap());
        println!("{}", path);
        proc_desc.write_to_file(&path).unwrap();
        let _ = ProcDesc::from_path(&path).unwrap();
    }
}