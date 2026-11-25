use crate::type_family::{TypeFamily, TypeName};

use crate::data_type_param_array::DataTypeParamArray;
use crate::data_type_param_kind::DataTypeParamKind;
use crate::data_type_param_numeric::DataTypeParamNumeric;
use crate::data_type_param_string::DataTypeParamString;
use crate::data_type_param_struct::DataTypeParamStruct;
use crate::data_type_param_time::DataTypeParamTime;
use crate::data_type_param_timestamp::DataTypeParamTimestamp;
use crate::data_type_param_timestamptz::DataTypeParamTimestampTz;
use paste::paste;
use serde::{Deserialize, Serialize};

/// A concrete data type: a type family paired with an optional parameter
/// object (for example `varchar(42)` is family `String` with a string
/// parameter).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataType {
    pub(crate) id: TypeFamily,
    pub(crate) param: Option<DataTypeParamKind<DataType>>,
}

unsafe impl Send for DataType {}

unsafe impl Sync for DataType {}

impl DataType {
    /// Returns the type family of this type.
    pub fn type_family(&self) -> TypeFamily {
        self.id
    }

    /// Creates a type of family `id` without a parameter object.
    pub fn new_no_param(id: TypeFamily) -> DataType {
        Self { id, param: None }
    }

    /// Returns true if this type carries no parameter object.
    pub fn has_no_param(&self) -> bool {
        self.param.is_none()
    }

    /// Parses a type from its serializable descriptor.
    /// Creates a type from a type family and an explicit parameter object.
    pub fn from_id_param(
        type_family: TypeFamily,
        param: Option<DataTypeParamKind<DataType>>,
    ) -> Self {
        Self {
            id: type_family,
            param,
        }
    }

    /// Returns the SQL name of this type (for example `varchar`).
    pub fn name(&self) -> String {
        if self.id.is_scalar_type() {
            self.id.name().to_string()
        } else {
            match &self.param {
                None => self.id.name().to_string(),
                Some(p) => p.name(),
            }
        }
    }
}

impl TypeName for DataType {
    fn name(&self) -> String {
        DataType::name(self)
    }
}

#[cfg(test)]
mod tests {
    use super::DataType;
    use crate::data_type_codec::TypeCodec;
    use crate::data_type_param_numeric::DataTypeParamNumeric;
    use crate::data_type_param_string::DataTypeParamString;
    use crate::data_type_param_time::DataTypeParamTime;
    use crate::data_type_param_timestamp::DataTypeParamTimestamp;
    use crate::data_type_param_timestamptz::DataTypeParamTimestampTz;
    use crate::type_family::TypeFamily;

    #[test]
    fn info_roundtrip_no_param_types() {
        let ids = [
            TypeFamily::I32,
            TypeFamily::I64,
            TypeFamily::F32,
            TypeFamily::F64,
            TypeFamily::U128,
            TypeFamily::I128,
            TypeFamily::Date,
        ];
        for id in ids {
            let original = DataType::new_no_param(id);
            let info = original.to_info();
            let restored = DataType::from_info(&info).unwrap();
            assert_eq!(restored.type_family(), id);
            assert!(restored.has_no_param());
        }
    }

    #[test]
    fn info_roundtrip_parameterized_types() {
        let original = DataType::from_string(DataTypeParamString::new(42));
        let info = original.to_info();
        let restored = DataType::from_info(&info).unwrap();
        assert_eq!(restored.type_family(), TypeFamily::String);
        assert_eq!(restored.as_string_param().unwrap().length(), 42);

        let original = DataType::from_numeric(DataTypeParamNumeric::new(10, 2));
        let info = original.to_info();
        let restored = DataType::from_info(&info).unwrap();
        assert_eq!(restored.type_family(), TypeFamily::Numeric);
        let param = restored.as_numeric_param().unwrap();
        assert_eq!(param.precision(), 10);
        assert_eq!(param.scale(), 2);

        let original = DataType::from_time(DataTypeParamTime::new(3));
        let info = original.to_info();
        let restored = DataType::from_info(&info).unwrap();
        assert_eq!(restored.type_family(), TypeFamily::Time);
        assert_eq!(restored.as_time_param().unwrap().precision(), 3);

        let original = DataType::from_timestamp(DataTypeParamTimestamp::new(4));
        let info = original.to_info();
        let restored = DataType::from_info(&info).unwrap();
        assert_eq!(restored.type_family(), TypeFamily::Timestamp);
        assert_eq!(restored.as_timestamp_param().unwrap().precision(), 4);

        let original = DataType::from_timestamptz(DataTypeParamTimestampTz::new(5));
        let info = original.to_info();
        let restored = DataType::from_info(&info).unwrap();
        assert_eq!(restored.type_family(), TypeFamily::TimestampTz);
        assert_eq!(restored.as_timestamptz_param().unwrap().precision(), 5);
    }

    #[test]
    fn name_returns_expected_strings() {
        assert_eq!(DataType::new_no_param(TypeFamily::I32).name(), "int");
        assert_eq!(DataType::new_no_param(TypeFamily::I64).name(), "bigint");
        assert_eq!(DataType::new_no_param(TypeFamily::F32).name(), "float");
        assert_eq!(DataType::new_no_param(TypeFamily::F64).name(), "double");
        assert_eq!(DataType::new_no_param(TypeFamily::Date).name(), "date");
        assert_eq!(DataType::new_no_param(TypeFamily::U128).name(), "oid");
        assert_eq!(DataType::new_no_param(TypeFamily::I128).name(), "i128");
        assert_eq!(
            DataType::from_numeric(DataTypeParamNumeric::new(10, 2)).name(),
            "numeric"
        );
        assert_eq!(
            DataType::from_string(DataTypeParamString::new(100)).name(),
            "varchar"
        );
        assert_eq!(
            DataType::from_time(DataTypeParamTime::new(3)).name(),
            "time"
        );
        assert_eq!(
            DataType::from_timestamp(DataTypeParamTimestamp::new(4)).name(),
            "timestamp"
        );
        assert_eq!(
            DataType::from_timestamptz(DataTypeParamTimestampTz::new(5)).name(),
            "timestamptz"
        );
    }

    #[test]
    fn default_for_scalar_types() {
        assert_eq!(
            DataType::default_for(TypeFamily::I32).type_family(),
            TypeFamily::I32
        );
        assert_eq!(
            DataType::default_for(TypeFamily::I64).type_family(),
            TypeFamily::I64
        );
        assert_eq!(
            DataType::default_for(TypeFamily::Numeric).type_family(),
            TypeFamily::Numeric
        );
        assert_eq!(
            DataType::default_for(TypeFamily::String).type_family(),
            TypeFamily::String
        );
        assert_eq!(
            DataType::default_for(TypeFamily::Time).type_family(),
            TypeFamily::Time
        );
    }

    #[test]
    #[should_panic(expected = "DataType::default_for(Array)")]
    fn default_for_array_panics() {
        let _ = DataType::default_for(TypeFamily::Array);
    }

    #[test]
    #[should_panic(expected = "DataType::default_for(Struct)")]
    fn default_for_record_panics() {
        let _ = DataType::default_for(TypeFamily::Struct);
    }

    #[test]
    #[should_panic(expected = "DataType::default_for(Binary)")]
    fn default_for_binary_panics() {
        let _ = DataType::default_for(TypeFamily::Binary);
    }
}

macro_rules! impl_data_type_methods {
    ($((
        $inner_type:ty,
        $variant_upper:ident,
        $variant_lower:ident
    )),+ $(,)?) => {
        $(
            paste! {
                impl DataType {
                    #[doc = "Constructor for type `"]
                    #[doc = stringify!($inner_type)]
                    #[doc = "`"]
                    pub fn [<from_ $variant_lower>](value: $inner_type) -> Self {
                        Self::from_id_param(TypeFamily::$variant_upper, Some(DataTypeParamKind::$variant_upper(Box::new(value))))
                    }

                    #[doc = "Get reference to internal type`"]
                    #[doc = stringify!($inner_type)]
                    #[doc = "` value"]
                    pub fn [<as_ $variant_lower _param>](&self) -> Option<&$inner_type> {
                        match &self.param {
                            Some(DataTypeParamKind::$variant_upper(v)) => Some(v.as_ref()),
                            _ => None,
                        }
                    }

                    #[doc = "Expect get reference to internal `"]
                    #[doc = stringify!($inner_type)]
                    #[doc = "` value"]
                    pub fn [<expect_ $variant_lower _param>](&self) -> &$inner_type {
                        self.[<as_ $variant_lower _param>]().unwrap()
                    }

                    #[doc = "Into internal `"]
                    #[doc = stringify!($inner_type)]
                    #[doc = "` value"]
                    pub fn [<into_ $variant_lower _param>](self) -> $inner_type {
                        match self.param {
                            Some(DataTypeParamKind::$variant_upper(v)) => *v,
                            _ => unsafe { std::hint::unreachable_unchecked() },
                        }
                    }
                }
            }
        )+
    };
}

impl Default for DataType {
    fn default() -> Self {
        DataType::from_id_param(TypeFamily::I32, None)
    }
}

impl_data_type_methods! {
    (DataTypeParamString, String, string),
    (DataTypeParamNumeric, Numeric, numeric),
    (DataTypeParamTime, Time, time),
    (DataTypeParamTimestamp, Timestamp, timestamp),
    (DataTypeParamTimestampTz, TimestampTz, timestamptz),
    (DataTypeParamStruct<DataType>, Struct, struct),
    (DataTypeParamArray<DataType>, Array, array),
}
