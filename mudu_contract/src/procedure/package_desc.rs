use crate::procedure::proc_desc::ProcDesc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use mudu::utils::json::to_json_str;

#[derive(Serialize, Deserialize, Clone)]
pub struct PackageDesc {
    /// module name to procedure description
    modules: HashMap<String, Vec<ProcDesc>>,
}

impl PackageDesc {
    pub fn new_empty() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    pub fn new(modules: HashMap<String, Vec<ProcDesc>>) -> Self {
        Self {
            modules,
        }
    }

    pub fn modules(&self) -> &HashMap<String, Vec<ProcDesc>> {
        &self.modules
    }

    pub fn into_modules(self) -> HashMap<String, Vec<ProcDesc>> {
        self.modules
    }

    pub fn add(&mut self, desc: ProcDesc) {
        if let Some(vec) = self.modules.get_mut(desc.module_name()) {
            vec.push(desc);
        } else {
            self.modules
                .insert(desc.module_name().to_string(), vec![desc]);
        }
    }

    pub fn merge(&mut self, other: &mut Self) {
        let mut other_modules = Default::default();
        std::mem::swap(&mut other_modules, &mut other.modules);
        for (name, other_desc_list) in other_modules {
            if let Some(desc_list) = self.modules.get_mut(&name) {
                desc_list.extend(other_desc_list);
            } else {
                self.modules.insert(name, other_desc_list);
            }
        }
    }
}

impl Display for PackageDesc {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = to_json_str(self).map_err(|_e| std::fmt::Error)?;
        std::fmt::Display::fmt(&s, f)?;
        Ok(())
    }
}

impl Debug for PackageDesc {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use crate::procedure::package_desc::PackageDesc;
    use crate::procedure::proc_desc::ProcDesc;
    use crate::tuple::tuple_datum::TupleDatum;
    use mudu::common::result::RS;
    use mudu::utils::json::{read_json, write_json};
    use std::collections::HashMap;
    use std::env::temp_dir;
    use uuid::Uuid;

    #[test]
    fn test_app_proc_desc() {
        _test_app_proc_desc().unwrap()
    }

    fn _test_app_proc_desc() -> RS<()> {
        let mut map = HashMap::new();
        for j in 0..2 {
            let mod_name = format!("mod_{}", j);
            let mut vec = vec![];
            for i in 0..3 {
                let param_desc = <(i32, i32, i64)>::tuple_desc_static(&[]);
                let return_desc = <(i32, String)>::tuple_desc_static(&[]);
                let proc_desc = ProcDesc::new(
                    mod_name.clone(),
                    format!("proc_{}", i),
                    param_desc,
                    return_desc,
                    false,
                );
                vec.push(proc_desc);
            }
            map.insert(mod_name, vec);
        }
        let app_proc_desc = PackageDesc { modules: map };
        let id = Uuid::new_v4().to_string();
        let path = format!("{}/proc_desc_{}.toml", temp_dir().to_str().unwrap(), id);

        println!("{}", path);
        write_json(&app_proc_desc, &path)?;

        let app_proc_desc1: PackageDesc = read_json(&path)?;
        println!("{}", app_proc_desc1);
        Ok(())
    }
}
