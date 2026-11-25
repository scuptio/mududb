//! Serialization codec for [`DataType`]: converts between the concrete type
//! and its serializable descriptor [`DataTypeInfo`].
//!
//! This layer depends on the core type definition, the parameter factory
//! table and the descriptor; keeping it separate lets the core [`DataType`]
//! definition stay free of any serialization or factory-table dependency.

use crate::data_type::DataType;
use crate::data_type_impl::data_type_table::get_fn_param;
use crate::data_type_info::DataTypeInfo;
use crate::type_error::TyErr;
use crate::type_family::TypeFamily;
use crate::type_family_ext::TypeFamilyExt;
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::cmp::Ordering;

/// Serialization/deserialization between a concrete type and its serializable
/// descriptor [`DataTypeInfo`]. Kept here (rather than on the core type) so the
/// [`DataType`] definition stays free of serialization and factory-table
/// dependencies.
pub trait TypeCodec: Sized {
    /// Returns the serializable descriptor of this type.
    fn to_info(&self) -> DataTypeInfo;

    /// Parses a type from its serializable descriptor.
    fn from_info(info: &DataTypeInfo) -> Result<Self, TyErr>;
}

impl TypeCodec for DataType {
    fn to_info(&self) -> DataTypeInfo {
        DataTypeInfo {
            id: self.id,
            param: self.param.as_ref().map_or(Default::default(), |p| {
                p.map(|dt_p| dt_p.se_to_json().unwrap())
            }),
        }
    }

    fn from_info(info: &DataTypeInfo) -> Result<Self, TyErr> {
        let opt_param = get_fn_param(info.id.to_u32());
        if let Some(fn_param) = opt_param {
            (fn_param.input)(&info.param)
        } else {
            Ok(DataType::new_no_param(info.id))
        }
    }
}

/// Creates a serializable descriptor from a type object (previously
/// `DataTypeInfo::from_opt_object`).
pub fn from_opt_object(param: &DataType) -> DataTypeInfo {
    <DataType as TypeCodec>::to_info(param)
}

/// Parses a descriptor back into a concrete type object (previously
/// `DataTypeInfo::to_data_type`).
pub fn to_data_type(info: &DataTypeInfo) -> RS<DataType> {
    <DataType as TypeCodec>::from_info(info)
        .map_err(|_e| mudu_error!(ErrorCode::TypeConversionFailed, "parse parameter error"))
}

impl DataType {
    /// Creates the type of family `id` with its default parameter.
    ///
    /// Panics if `id` is not a scalar type.
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
