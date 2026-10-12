//! Collection of procedure descriptions grouped by module name.

use crate::procedure::proc_desc::ProcDesc;
use mudu::utils::json::to_json_str;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};

/// Procedure descriptions of all modules in an application.
#[derive(Serialize, Deserialize, Clone)]
pub struct ModProcDesc {
    /// module name to procedure description
    modules: HashMap<String, Vec<ProcDesc>>,
}

impl ModProcDesc {
    /// Creates an empty collection.
    pub fn new_empty() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    /// Creates a collection from a module name to procedure descriptions map.
    pub fn new(modules: HashMap<String, Vec<ProcDesc>>) -> Self {
        Self { modules }
    }

    /// Returns the module name to procedure descriptions map.
    pub fn modules(&self) -> &HashMap<String, Vec<ProcDesc>> {
        &self.modules
    }

    /// Consumes `self` and returns the module name to procedure descriptions map.
    pub fn into_modules(self) -> HashMap<String, Vec<ProcDesc>> {
        self.modules
    }

    /// Adds a procedure description under its module name.
    pub fn add(&mut self, desc: ProcDesc) {
        if let Some(vec) = self.modules.get_mut(desc.module_name()) {
            vec.push(desc);
        } else {
            self.modules
                .insert(desc.module_name().to_string(), vec![desc]);
        }
    }

    /// Merges all procedure descriptions from `other` into `self`, leaving `other` empty.
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

impl Display for ModProcDesc {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = to_json_str(self).map_err(|_e| std::fmt::Error)?;
        std::fmt::Display::fmt(&s, f)?;
        Ok(())
    }
}

impl Debug for ModProcDesc {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}
