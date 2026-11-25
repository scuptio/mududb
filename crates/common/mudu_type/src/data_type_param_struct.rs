use crate::data_type_param::{DTPStatic, DataTypeParamDyn};
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use mudu::utils;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::Debug;

/// The parameter of the `Struct` (`object`) type: the record name and its
/// named, typed fields.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DataTypeParamStruct<T> {
    name: String,
    // field name and its data kind
    field: Vec<(String, T)>,
}

impl<T: Clone + Serialize + DeserializeOwned + Debug + Send + Sync + 'static> DataTypeParamDyn
    for DataTypeParamStruct<T>
{
    fn clone_boxed(&self) -> Box<dyn DataTypeParamDyn> {
        Box::new(self.clone())
    }

    fn de_from_json(&mut self, json: &str) -> RS<()> {
        let s: Self = utils::json::from_json_str::<Self>(json)?;
        self.name = s.name;
        self.field = s.field;
        Ok(())
    }

    fn se_to_json(&self) -> RS<String> {
        utils::json::to_json_str(&self)
    }

    fn name(&self) -> String {
        self.name.clone()
    }
}

impl<T: Clone> DataTypeParamStruct<T> {
    /// Creates a record parameter with a type name and named, typed fields.
    pub fn new(name: String, field: Vec<(String, T)>) -> DataTypeParamStruct<T> {
        let mut map = HashMap::new();
        for (name, ty) in &field {
            map.insert(name.clone(), ty.clone());
        }
        Self { name, field }
    }
}

impl<T> DataTypeParamStruct<T> {
    /// Returns the record type name.
    pub fn struct_name(&self) -> &String {
        &self.name
    }

    /// Returns the field names and their data types.
    pub fn fields(&self) -> &Vec<(String, T)> {
        &self.field
    }

    /// Consumes the parameter and returns the record name and fields.
    pub fn into(self) -> (String, Vec<(String, T)>) {
        (self.name, self.field)
    }

    fn compare(&self, other: &Self) -> Ordering {
        if self.name.eq(&other.name) {
            Ordering::Equal
        } else {
            self.field.len().cmp(&other.field.len())
        }
    }
}

impl<T> Order for DataTypeParamStruct<T> {
    fn cmp_ord(&self, other: &Self) -> RS<Ordering> {
        Ok(self.compare(other))
    }
}

impl<T: Clone + Serialize + DeserializeOwned + Debug + Send + Sync + 'static> DTPStatic
    for DataTypeParamStruct<T>
{
}
