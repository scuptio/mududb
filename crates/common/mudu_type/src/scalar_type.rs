use crate::data_type::DataType;
use crate::data_type_info::DataTypeInfo;
use crate::type_family::TypeFamily;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A scalar (non-composite) data type.
///
/// Wraps a [`DataType`] whose type family is scalar; construction panics if
/// given a non-scalar family such as `Array`, `Record`, or `Binary`.
#[derive(Clone, Debug)]
pub struct ScalarType {
    data_type: DataType,
}

impl ScalarType {
    /// Creates a scalar type of family `id` without a parameter object.
    ///
    /// Panics if `id` is not a scalar type.
    pub fn new_without_param(id: TypeFamily) -> Self {
        if !id.is_scalar_type() {
            panic!("ScalarType id must be scalar type, but got {}", id.name());
        }
        Self {
            data_type: DataType::new_no_param(id),
        }
    }

    /// Creates a scalar type of family `id` with its default parameter.
    ///
    /// Panics if `id` is not a scalar type.
    pub fn new_default(id: TypeFamily) -> Self {
        if !id.is_scalar_type() {
            panic!("ScalarType id must be scalar type, but got {}", id.name());
        }
        Self::new(DataType::default_for(id))
    }

    /// Wraps a concrete type object as a scalar type.
    ///
    /// Panics if the type family of `type_obj` is not scalar.
    pub fn new(type_obj: DataType) -> Self {
        if !type_obj.type_family().is_scalar_type() {
            panic!(
                "ScalarType id must be scalar type, but got {}",
                type_obj.type_family().name()
            );
        }
        Self {
            data_type: type_obj,
        }
    }

    /// Returns the type family of this scalar type.
    pub fn id(&self) -> TypeFamily {
        self.data_type.type_family()
    }

    /// Returns the underlying type object.
    pub fn type_obj(&self) -> &DataType {
        &self.data_type
    }

    /// Returns true if this scalar type carries a parameter object.
    pub fn has_param(&self) -> bool {
        !self.data_type.has_no_param()
    }

    /// Returns the serializable descriptor of this scalar type.
    pub fn param_info(&self) -> DataTypeInfo {
        self.data_type.to_info()
    }
}

impl<'de> Deserialize<'de> for ScalarType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let obj: DataType = Deserialize::deserialize(deserializer)?;
        Ok(Self { data_type: obj })
    }
}

impl Serialize for ScalarType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_some(&self.data_type)
    }
}
