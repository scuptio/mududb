use crate::type_family::TypeFamily;

use crate::data_type_impl::data_type_table::get_fn_param;

use crate::data_type_info::DataTypeInfo;
use crate::data_type_param_array::DataTypeParamArray;
use crate::data_type_param_kind::DataTypeParamKind;
use crate::data_type_param_numeric::DataTypeParamNumeric;
use crate::data_type_param_record::DataTypeParamRecord;
use crate::data_type_param_string::DataTypeParamString;
use crate::data_type_param_time::DataTypeParamTime;
use crate::data_type_param_timestamp::DataTypeParamTimestamp;
use crate::data_type_param_timestamptz::DataTypeParamTimestampTz;
use crate::type_error::TyErr;
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use paste::paste;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Data type param object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataType {
    id: TypeFamily,
    param: Option<DataTypeParamKind>,
}

unsafe impl Send for DataType {}

unsafe impl Sync for DataType {}

impl DataType {
    pub fn default_for(id: TypeFamily) -> DataType {
        if !id.is_scalar_type() {
            panic!("DataType::default_for({:?})", id);
        }
        let opt = id.opt_fn_param();

        match opt {
            Some(t) => {
                if let Some(d) = t.default {
                    d()
                } else {
                    DataType::new_no_param(id)
                }
            }
            None => DataType::new_no_param(id),
        }
    }

    pub fn type_family(&self) -> TypeFamily {
        self.id
    }

    pub fn new_no_param(id: TypeFamily) -> DataType {
        Self { id, param: None }
    }

    pub fn has_no_param(&self) -> bool {
        self.param.is_none()
    }

    pub fn from_info(info: &DataTypeInfo) -> Result<Self, TyErr> {
        let opt_param = get_fn_param(info.id.to_u32());
        if let Some(fn_param) = opt_param {
            (fn_param.input)(&info.param)
        } else {
            Ok(Self {
                id: info.id,
                param: None,
            })
        }
    }

    pub fn new(info: &DataTypeInfo) -> Result<Self, TyErr> {
        Self::from_info(info)
    }

    pub fn from_id_param(type_family: TypeFamily, param: Option<DataTypeParamKind>) -> Self {
        Self {
            id: type_family,
            param,
        }
    }

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

    pub fn into_info(self) -> DataTypeInfo {
        DataTypeInfo {
            id: self.id,
            param: self.param.map_or(Default::default(), |p| {
                p.map(|dt_p| dt_p.se_to_json().unwrap())
            }),
        }
    }

    pub fn to_info(&self) -> DataTypeInfo {
        DataTypeInfo {
            id: self.id,
            param: self.param.as_ref().map_or(Default::default(), |p| {
                p.map(|dt_p| dt_p.se_to_json().unwrap())
            }),
        }
    }

    fn compare(&self, other: &DataType) -> RS<Ordering> {
        let ord = if !self.id.has_param() && !other.id.has_param() {
            self.id.cmp(&other.id)
        } else {
            let opt_len1 = self.id.fn_send_type_len()(self).map_err(|e| e.to_m_err())?;
            let opt_len2 = other.id.fn_send_type_len()(other).map_err(|e| e.to_m_err())?;
            // fixed length type come first
            match (opt_len1, opt_len2) {
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(_), Some(_)) => self.compare_inner(other)?,
                (None, None) => self.compare_inner(other)?,
            }
        };
        Ok(ord)
    }

    fn compare_inner(&self, other: &DataType) -> RS<Ordering> {
        let ord = self.id.cmp(&other.id);
        if ord != Ordering::Equal {
            Ok(ord)
        } else {
            let ord = match (&self.param, &other.param) {
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
                (Some(k1), Some(k2)) => k1.cmp_ord(k2)?,
                (None, None) => Ordering::Equal,
            };
            Ok(ord)
        }
    }
}

impl Order for DataType {
    fn cmp_ord(&self, other: &Self) -> RS<Ordering> {
        self.compare(other)
    }
}

#[cfg(test)]
mod tests {
    use super::DataType;
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
    #[should_panic(expected = "DataType::default_for(Record)")]
    fn default_for_record_panics() {
        let _ = DataType::default_for(TypeFamily::Record);
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
    (DataTypeParamRecord, Record, record),
    (DataTypeParamArray, Array, array),
}
