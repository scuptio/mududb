use crate::procedure::proc_desc::ProcDesc;
use crate::utils::toml::to_toml_str;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};

#[derive(Serialize, Deserialize, Clone)]
pub struct AppProcDesc {
    /// module name to procedure description
    pub modules: HashMap<String, Vec<ProcDesc>>,
}

impl AppProcDesc {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    pub fn add(&mut self, desc: ProcDesc) {
        if let Some(vec) = self.modules.get_mut(desc.module_name()) {
            vec.push(desc);
        } else {
            self.modules
                .insert(desc.module_name().to_string(), vec![desc]);
        }
    }
}

impl Display for AppProcDesc {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = to_toml_str(self).map_err(|_e| std::fmt::Error)?;
        std::fmt::Display::fmt(&s, f)?;
        Ok(())
    }
}

impl Debug for AppProcDesc {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use crate::common::result::RS;
    use crate::procedure::proc_desc::ProcDesc;
    use crate::tuple::rs_tuple_datum::RsTupleDatum;
    use crate::utils::app_proc_desc::AppProcDesc;
    use crate::utils::toml::{read_toml, write_toml};
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
                let param_desc = <(i32, i32, i64)>::tuple_desc_static();
                let return_desc = <(i32, String)>::tuple_desc_static();
                let proc_desc = ProcDesc::new(
                    mod_name.clone(),
                    format!("proc_{}", i),
                    param_desc,
                    return_desc,
                );
                vec.push(proc_desc);
            }
            map.insert(mod_name, vec);
        }
        let app_proc_desc = AppProcDesc { modules: map };
        let id = Uuid::new_v4().to_string();
        let path = format!("{}/proc_desc_{}.toml", temp_dir().to_str().unwrap(), id);

        println!("{}", path);
        write_toml(&app_proc_desc, &path)?;

        let app_proc_desc1: AppProcDesc = read_toml(&path)?;
        println!("{}", app_proc_desc1);
        Ok(())
    }
}
