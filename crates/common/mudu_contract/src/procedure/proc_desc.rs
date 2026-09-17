//! `procedure::proc_desc` module.
#![allow(missing_docs)]

use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::json::JsonValue;
use serde::{Deserialize, Serialize};
use serde_json::Map;
use serde_json::Value;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

/// Describes a procedure's interface including parameter and return types
/// Used for procedure signature validation and serialization
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcDesc {
    module_name: String,         // Name of the module containing the procedure
    proc_name: String,           // Name of the procedure
    param_desc: TupleFieldDesc,  // Description of procedure parameters
    return_desc: TupleFieldDesc, // Description of procedure return values
}

impl ProcDesc {
    /// Creates a new procedure description
    pub fn new(
        module_name: String,
        proc_name: String,
        param_desc: TupleFieldDesc,
        return_desc: TupleFieldDesc,
        _is_async: bool,
    ) -> ProcDesc {
        Self {
            proc_name,
            module_name,
            param_desc,
            return_desc,
        }
    }

    // Getters for accessing private fields

    /// Returns the module name
    pub fn module_name(&self) -> &String {
        &self.module_name
    }

    pub fn is_async(&self) -> bool {
        false
    }

    /// Returns the procedure name
    pub fn proc_name(&self) -> &String {
        &self.proc_name
    }

    /// Returns the parameter type description
    pub fn param_desc(&self) -> &TupleFieldDesc {
        &self.param_desc
    }

    /// Returns the return type description
    pub fn return_desc(&self) -> &TupleFieldDesc {
        &self.return_desc
    }

    /// Serializes the procedure description to a formatted TOML string
    pub fn to_toml_str(&self) -> RS<String> {
        #[allow(clippy::unwrap_used)]
        Ok(toml::to_string_pretty(&self).unwrap())
    }

    /// Writes the procedure description to a file as TOML
    #[cfg(not(target_arch = "wasm32"))]
    pub fn write_to_file<P: AsRef<Path>>(&self, path: P) -> RS<()> {
        let s = self.to_toml_str()?;
        mudu_sys::fs::sync::sync_write(path.as_ref(), s.as_bytes())?;
        Ok(())
    }

    /// Reads and deserializes a procedure description from a TOML file
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_path<P: AsRef<Path>>(path: P) -> RS<Self> {
        let s = mudu_sys::fs::sync::sync_read_to_string(path.as_ref())?;
        let ret: Self = toml::from_str::<Self>(&s)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode from toml string error", e))?;
        Ok(ret)
    }

    /// Generate arbitrary parameter values as JSON map
    pub fn default_param_json(&self) -> RS<JsonValue> {
        let map = self.generate_default_map(&self.param_desc)?;
        Ok(JsonValue::Object(map))
    }

    /// Generate arbitrary return values as JSON map
    pub fn default_return_json(&self) -> RS<JsonValue> {
        let map = self.generate_default_map(&self.return_desc)?;
        Ok(JsonValue::Object(map))
    }

    /// Generate default value for a specific DatumDesc
    fn generate_default_value(&self, desc: &DatumDesc) -> RS<(String, Value)> {
        // Get the datatype ID and corresponding FnArbitrary functions
        let obj = desc.data_type();

        let tp_id = obj.type_family();
        let data_internal = tp_id.fn_default()(obj).map_err(|e| {
            mudu_error!(
                ErrorCode::TypeConversionFailed,
                "error when generating default value",
                e
            )
        })?;
        #[allow(clippy::unwrap_used)]
        let dat_printable = tp_id.fn_output_json()(&data_internal, obj).unwrap();
        let value = dat_printable.into_json_value();
        Ok((desc.name().to_string(), value))
    }

    /// Generate default map based on TupleFieldDesc
    fn generate_default_map(&self, desc: &TupleFieldDesc) -> RS<Map<String, Value>> {
        let mut map = Map::new();
        for field in desc.fields() {
            let kv = self.generate_default_value(field)?;
            map.insert(kv.0, kv.1);
        }
        Ok(map)
    }
}
