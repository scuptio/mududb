use crate::dat_type_id::DatTypeID;

use crate::dt_impl::dat_table::get_fn_param;

use crate::dt_info::DTInfo;
use crate::dtp_array::DTPArray;
use crate::dtp_kind::DTPKind;
use crate::dtp_numeric::DTPNumeric;
use crate::dtp_object::DTPRecord;
use crate::dtp_string::DTPString;
use crate::dtp_time::DTPTime;
use crate::dtp_timestamp::DTPTimestamp;
use crate::dtp_timestamptz::DTPTimestampTz;
use crate::type_error::TyErr;
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use paste::paste;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Data type param object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatType {
    id: DatTypeID,
    param: Option<DTPKind>,
}

unsafe impl Send for DatType {}

unsafe impl Sync for DatType {}

impl DatType {
    pub fn default_for(id: DatTypeID) -> DatType {
        if !id.is_scalar_type() {
            panic!("DatType::default_for({:?})", id);
        }
        let opt = id.opt_fn_param();

        match opt {
            Some(t) => {
                if let Some(d) = t.default {
                    d()
                } else {
                    DatType::new_no_param(id)
                }
            }
            None => DatType::new_no_param(id),
        }
    }

    pub fn dat_type_id(&self) -> DatTypeID {
        self.id
    }

    pub fn new_no_param(id: DatTypeID) -> DatType {
        Self { id, param: None }
    }

    pub fn has_no_param(&self) -> bool {
        self.param.is_none()
    }

    pub fn from_info(info: &DTInfo) -> Result<Self, TyErr> {
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

    pub fn new(info: &DTInfo) -> Result<Self, TyErr> {
        Self::from_info(info)
    }

    pub fn from_id_param(dat_type_id: DatTypeID, param: Option<DTPKind>) -> Self {
        Self {
            id: dat_type_id,
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

    pub fn into_info(self) -> DTInfo {
        DTInfo {
            id: self.id,
            param: self.param.map_or(Default::default(), |p| {
                p.map(|dt_p| dt_p.se_to_json().unwrap())
            }),
        }
    }

    pub fn to_info(&self) -> DTInfo {
        DTInfo {
            id: self.id,
            param: self.param.as_ref().map_or(Default::default(), |p| {
                p.map(|dt_p| dt_p.se_to_json().unwrap())
            }),
        }
    }

    fn compare(&self, other: &DatType) -> RS<Ordering> {
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

    fn compare_inner(&self, other: &DatType) -> RS<Ordering> {
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

impl Order for DatType {
    fn cmp_ord(&self, other: &Self) -> RS<Ordering> {
        self.compare(other)
    }
}

#[cfg(test)]
mod tests {
    use super::DatType;
    use crate::dat_type_id::DatTypeID;
    use crate::dtp_numeric::DTPNumeric;
    use crate::dtp_string::DTPString;
    use crate::dtp_time::DTPTime;
    use crate::dtp_timestamp::DTPTimestamp;
    use crate::dtp_timestamptz::DTPTimestampTz;

    #[test]
    fn info_roundtrip_no_param_types() {
        let ids = [
            DatTypeID::I32,
            DatTypeID::I64,
            DatTypeID::F32,
            DatTypeID::F64,
            DatTypeID::U128,
            DatTypeID::I128,
            DatTypeID::Date,
        ];
        for id in ids {
            let original = DatType::new_no_param(id);
            let info = original.to_info();
            let restored = DatType::from_info(&info).unwrap();
            assert_eq!(restored.dat_type_id(), id);
            assert!(restored.has_no_param());
        }
    }

    #[test]
    fn info_roundtrip_parameterized_types() {
        let original = DatType::from_string(DTPString::new(42));
        let info = original.to_info();
        let restored = DatType::from_info(&info).unwrap();
        assert_eq!(restored.dat_type_id(), DatTypeID::String);
        assert_eq!(restored.as_string_param().unwrap().length(), 42);

        let original = DatType::from_numeric(DTPNumeric::new(10, 2));
        let info = original.to_info();
        let restored = DatType::from_info(&info).unwrap();
        assert_eq!(restored.dat_type_id(), DatTypeID::Numeric);
        let param = restored.as_numeric_param().unwrap();
        assert_eq!(param.precision(), 10);
        assert_eq!(param.scale(), 2);

        let original = DatType::from_time(DTPTime::new(3));
        let info = original.to_info();
        let restored = DatType::from_info(&info).unwrap();
        assert_eq!(restored.dat_type_id(), DatTypeID::Time);
        assert_eq!(restored.as_time_param().unwrap().precision(), 3);

        let original = DatType::from_timestamp(DTPTimestamp::new(4));
        let info = original.to_info();
        let restored = DatType::from_info(&info).unwrap();
        assert_eq!(restored.dat_type_id(), DatTypeID::Timestamp);
        assert_eq!(restored.as_timestamp_param().unwrap().precision(), 4);

        let original = DatType::from_timestamptz(DTPTimestampTz::new(5));
        let info = original.to_info();
        let restored = DatType::from_info(&info).unwrap();
        assert_eq!(restored.dat_type_id(), DatTypeID::TimestampTz);
        assert_eq!(restored.as_timestamptz_param().unwrap().precision(), 5);
    }

    #[test]
    fn name_returns_expected_strings() {
        assert_eq!(DatType::new_no_param(DatTypeID::I32).name(), "int");
        assert_eq!(DatType::new_no_param(DatTypeID::I64).name(), "bigint");
        assert_eq!(DatType::new_no_param(DatTypeID::F32).name(), "float");
        assert_eq!(DatType::new_no_param(DatTypeID::F64).name(), "double");
        assert_eq!(DatType::new_no_param(DatTypeID::Date).name(), "date");
        assert_eq!(DatType::new_no_param(DatTypeID::U128).name(), "oid");
        assert_eq!(DatType::new_no_param(DatTypeID::I128).name(), "i128");
        assert_eq!(
            DatType::from_numeric(DTPNumeric::new(10, 2)).name(),
            "numeric"
        );
        assert_eq!(DatType::from_string(DTPString::new(100)).name(), "varchar");
        assert_eq!(DatType::from_time(DTPTime::new(3)).name(), "time");
        assert_eq!(
            DatType::from_timestamp(DTPTimestamp::new(4)).name(),
            "timestamp"
        );
        assert_eq!(
            DatType::from_timestamptz(DTPTimestampTz::new(5)).name(),
            "timestamptz"
        );
    }

    #[test]
    fn default_for_scalar_types() {
        assert_eq!(
            DatType::default_for(DatTypeID::I32).dat_type_id(),
            DatTypeID::I32
        );
        assert_eq!(
            DatType::default_for(DatTypeID::I64).dat_type_id(),
            DatTypeID::I64
        );
        assert_eq!(
            DatType::default_for(DatTypeID::Numeric).dat_type_id(),
            DatTypeID::Numeric
        );
        assert_eq!(
            DatType::default_for(DatTypeID::String).dat_type_id(),
            DatTypeID::String
        );
        assert_eq!(
            DatType::default_for(DatTypeID::Time).dat_type_id(),
            DatTypeID::Time
        );
    }

    #[test]
    #[should_panic(expected = "DatType::default_for(Array)")]
    fn default_for_array_panics() {
        let _ = DatType::default_for(DatTypeID::Array);
    }

    #[test]
    #[should_panic(expected = "DatType::default_for(Record)")]
    fn default_for_record_panics() {
        let _ = DatType::default_for(DatTypeID::Record);
    }

    #[test]
    #[should_panic(expected = "DatType::default_for(Binary)")]
    fn default_for_binary_panics() {
        let _ = DatType::default_for(DatTypeID::Binary);
    }
}

macro_rules! impl_dat_type_methods {
    ($((
        $inner_type:ty,
        $variant_upper:ident,
        $variant_lower:ident
    )),+ $(,)?) => {
        $(
            paste! {
                impl DatType {
                    #[doc = "Constructor for type `"]
                    #[doc = stringify!($inner_type)]
                    #[doc = "`"]
                    pub fn [<from_ $variant_lower>](value: $inner_type) -> Self {
                        Self::from_id_param(DatTypeID::$variant_upper, Some(DTPKind::$variant_upper(Box::new(value))))
                    }

                    #[doc = "Get reference to internal type`"]
                    #[doc = stringify!($inner_type)]
                    #[doc = "` value"]
                    pub fn [<as_ $variant_lower _param>](&self) -> Option<&$inner_type> {
                        match &self.param {
                            Some(DTPKind::$variant_upper(v)) => Some(v.as_ref()),
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
                            Some(DTPKind::$variant_upper(v)) => *v,
                            _ => unsafe { std::hint::unreachable_unchecked() },
                        }
                    }
                }
            }
        )+
    };
}

impl Default for DatType {
    fn default() -> Self {
        DatType::from_id_param(DatTypeID::I32, None)
    }
}

impl_dat_type_methods! {
    (DTPString, String, string),
    (DTPNumeric, Numeric, numeric),
    (DTPTime, Time, time),
    (DTPTimestamp, Timestamp, timestamp),
    (DTPTimestampTz, TimestampTz, timestamptz),
    (DTPRecord, Record, record),
    (DTPArray, Array, array),
}
