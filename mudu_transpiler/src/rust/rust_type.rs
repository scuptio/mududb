//! Rust type representation used by the transpiler.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_type_desc::UniTypeDesc;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dtp_array::DTPArray;

/// A Rust type encountered in a procedure signature.
#[derive(Debug, Clone)]
pub enum RustType {
    Primitive(String),
    Tuple(Vec<RustType>),
    Custom(String),
    Generic(String, Vec<RustType>),
}

impl RustType {
    /// Return whether this type is `Vec<u8>`.
    pub fn is_vec_u8(&self) -> bool {
        match self {
            RustType::Generic(ident, vec) if ident == "Vec" && vec.len() == 1 => {
                matches!(&vec[0], RustType::Primitive(inner) if inner == "u8")
            }
            _ => false,
        }
    }

    /// Decompose a return type of the form `RS<(...)>` into its inner types.
    pub fn as_ret_type(&self) -> RS<Vec<RustType>> {
        match self {
            RustType::Generic(_, vec) => {
                if vec.len() != 1 {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        "RustType::as_ret_type, return type must be RS<(...)>"
                    ));
                }
                Ok(vec[0].as_ret_type_inner())
            }
            _ => Err(mudu_error!(
                ErrorCode::InvalidType,
                "RustType::as_ret_type, return type must be RS<(...)>"
            )),
        }
    }

    /// Render the type as a Rust source string.
    pub fn to_type_str(&self) -> String {
        match self {
            RustType::Primitive(s) => s.clone(),
            RustType::Tuple(vec) => {
                let mut s = "(".to_string();
                for t in vec.iter() {
                    s.push_str(t.to_type_str().as_str());
                    s.push_str(", ");
                }
                s.push(')');
                s
            }
            RustType::Custom(s) => s.clone(),
            RustType::Generic(s, vec) => {
                let mut s = format!("{}<", s);
                for t in vec.iter() {
                    s.push_str(t.to_type_str().as_str());
                    s.push_str(", ");
                }
                s.push('>');
                s
            }
        }
    }

    /// Render the inner types of an `RS<(...)>` return type as strings.
    pub fn to_ret_type_str(&self) -> RS<Vec<String>> {
        match self {
            RustType::Generic(_, vec) => {
                if vec.len() != 1 {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        "RustType::to_ret_type_str, return type must be RS<(...)>"
                    ));
                }
                Ok(vec[0].to_ret_type_str_inner())
            }
            _ => Err(mudu_error!(
                ErrorCode::InvalidType,
                "RustType::to_ret_type_str, return type must be RS<(...)>"
            )),
        }
    }

    fn to_ret_type_str_inner(&self) -> Vec<String> {
        match self {
            RustType::Primitive(s) => {
                vec![s.clone()]
            }
            RustType::Tuple(vec) => vec.iter().map(|t| t.to_type_str().clone()).collect(),
            _ => {
                vec![self.to_type_str()]
            }
        }
    }

    fn as_ret_type_inner(&self) -> Vec<RustType> {
        match &self {
            RustType::Primitive(_) => {
                vec![self.clone()]
            }
            RustType::Tuple(vec) => (*vec).clone(),
            _ => {
                vec![self.clone()]
            }
        }
    }

    /// Convert this Rust type to a Mudu [`DatType`].
    pub fn to_dat_type(&self, custom_types: &UniTypeDesc) -> RS<DatType> {
        let dat_type = match self {
            RustType::Primitive(s) => match s.as_str() {
                "i32" => DatType::default_for(DatTypeID::I32),
                "i64" => DatType::default_for(DatTypeID::I64),
                "i128" => DatType::default_for(DatTypeID::I128),
                "u128" => DatType::default_for(DatTypeID::U128),
                "f32" => DatType::default_for(DatTypeID::F32),
                "f64" => DatType::default_for(DatTypeID::F64),
                _ => {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        format!("not support type {}", s)
                    ));
                }
            },
            RustType::Custom(s) => match s.as_str() {
                "OID" => DatType::default_for(DatTypeID::U128),
                "String" => DatType::default_for(DatTypeID::String),
                _ => {
                    let ty = custom_types.types.get(s).map_or_else(
                        || {
                            Err(mudu_error!(
                                ErrorCode::EntityNotFound,
                                format!("no such type name:{}", s)
                            ))
                        },
                        Ok,
                    )?;
                    ty.clone().uni_to()?
                }
            },
            RustType::Generic(ident, vec) => {
                if self.is_vec_u8() {
                    DatType::new_no_param(DatTypeID::Binary)
                } else if ident == "Vec" && vec.len() == 1 {
                    let array = DTPArray::new(vec[0].to_dat_type(custom_types)?);
                    DatType::from_array(array)
                } else {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        format!("not support type {:?}", self)
                    ));
                }
            }
            _ => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    format!("not support type {:?}", self)
                ));
            }
        };
        Ok(dat_type)
    }
}

#[cfg(test)]
#[path = "rust_type_test.rs"]
mod rust_type_test;
