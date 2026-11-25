use crate::data_type_param::{DTPStatic, DataTypeParamDyn};
use crate::type_family::TypeName;
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use mudu::utils;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt::Debug;

/// The parameter of the `Array` type: the element data type and an optional
/// maximum size.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataTypeParamArray<T> {
    data_type: T,
    max_size: Option<u32>,
}

impl<T> DataTypeParamArray<T> {
    /// Creates an array parameter with element type `data_type` and no
    /// maximum size.
    pub fn new(data_type: T) -> DataTypeParamArray<T> {
        Self {
            data_type,
            max_size: None,
        }
    }

    /// Returns the element data type.
    pub fn data_type(&self) -> &T {
        &self.data_type
    }

    /// Consumes the parameter and returns the element data type.
    pub fn into_data_type(self) -> T {
        self.data_type
    }
}

impl<T: Default> Default for DataTypeParamArray<T> {
    fn default() -> Self {
        Self {
            data_type: T::default(),
            max_size: None,
        }
    }
}

impl<T: Clone + Serialize + DeserializeOwned + TypeName + Debug + Send + Sync + 'static>
    DataTypeParamDyn for DataTypeParamArray<T>
{
    fn clone_boxed(&self) -> Box<dyn DataTypeParamDyn> {
        Box::new(self.clone())
    }

    fn de_from_json(&mut self, json: &str) -> RS<()> {
        let s = utils::json::from_json_str::<Self>(json)?;
        self.data_type = s.data_type;
        self.max_size = s.max_size;
        Ok(())
    }

    fn se_to_json(&self) -> RS<String> {
        utils::json::to_json_str(&self)
    }

    fn name(&self) -> String {
        format!("array<{}>", self.data_type.name())
    }
}

impl<T: Order> Order for DataTypeParamArray<T> {
    fn cmp_ord(&self, other: &Self) -> RS<Ordering> {
        self.data_type.cmp_ord(&other.data_type)
    }
}

impl<T: Clone + Serialize + DeserializeOwned + TypeName + Order + Debug + Send + Sync + 'static>
    DTPStatic for DataTypeParamArray<T>
{
}
