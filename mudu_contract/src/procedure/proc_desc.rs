use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::utils::json::JsonValue;
use serde::{Deserialize, Serialize};
use serde_json::Map;
use serde_json::Value;
use std::fs;
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
    pub fn to_toml_str(&self) -> String {
        toml::to_string_pretty(&self).unwrap()
    }

    /// Writes the procedure description to a file as TOML
    pub fn write_to_file<P: AsRef<Path>>(&self, path: P) -> RS<()> {
        let s = self.to_toml_str();
        fs::write(path, s).map_err(|e| m_error!(EC::IOErr, "write to file error", e))?;
        Ok(())
    }

    /// Reads and deserializes a procedure description from a TOML file
    pub fn from_path<P: AsRef<Path>>(path: P) -> RS<Self> {
        let s = fs::read_to_string(path).map_err(|e| m_error!(EC::IOErr, "read path error", e))?;
        let ret: Self = toml::from_str::<Self>(&s)
            .map_err(|e| m_error!(EC::DecodeErr, "decode from toml string error", e))?;
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
        let obj = desc.dat_type();

        let tp_id = obj.dat_type_id();
        let dat_internal = tp_id.fn_default()(obj)
            .map_err(|e| m_error!(EC::TypeBaseErr, "error when generating default value", e))?;
        let dat_printable = tp_id.fn_output_json()(&dat_internal, obj)
            .map_err(|e| m_error!(EC::TypeBaseErr, "error when converting to printable", e))?;
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

#[cfg(test)]
mod test {
    use crate::procedure::proc_desc::ProcDesc;
    use crate::tuple::tuple_datum::TupleDatum;
    use std::env::temp_dir;

    #[test]
    fn test_proc_desc_serialization() {
        // Create parameter and return type descriptions
        let param_desc = <(i32, i32, i64)>::tuple_desc_static(&[]);
        let return_desc = <(i32, String)>::tuple_desc_static(&[]);

        // Create procedure description
        let proc_desc = ProcDesc::new(
            "module".to_string(),
            "proc".to_string(),
            param_desc,
            return_desc,
            false,
        );

        // Test file serialization/deserialization
        let path = format!("{}/proc_desc.toml", temp_dir().to_str().unwrap());
        println!("Test file path: {}", path);

        // Write to file
        proc_desc.write_to_file(&path).unwrap();

        // Read from file and verify
        let loaded_desc = ProcDesc::from_path(&path).unwrap();
        println!("parameter:{}", loaded_desc.default_param_json().unwrap().to_string());
        println!("return:{}", loaded_desc.default_return_json().unwrap().to_string());
        // Clean up test file
        let _ = std::fs::remove_file(&path);
    }
}
