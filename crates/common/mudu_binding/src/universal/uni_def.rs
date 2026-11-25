use crate::universal::uni_data_type::UniDataType;
use std::fmt;
use std::fmt::{Debug, Display};

#[derive(Debug, Clone)]
pub struct UniClassDef {
    pub table_comments: String,
    pub class_name: String,
    pub table_key: Vec<StructField>,
    pub table_value: Vec<StructField>,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub rf_comments: String,
    pub rf_name: String,
    pub rf_type: UniDataType,
    /// 1-based field number in WIT declaration order (protobuf-style field
    /// number). 0 means "not assigned": construction sites leave it unset and
    /// the WIT parser assigns numbers after parsing completes.
    pub rf_number: u32,
}

impl StructField {
    /// Create a field with an unassigned (0) field number.
    pub fn new(rf_comments: String, rf_name: String, rf_type: UniDataType) -> Self {
        Self {
            rf_comments,
            rf_name,
            rf_type,
            rf_number: 0,
        }
    }
}

/// Assign 1-based field numbers in declaration order.
pub fn assign_field_numbers(fields: &mut [StructField]) {
    for (i, field) in fields.iter_mut().enumerate() {
        field.rf_number = (i + 1) as u32;
    }
}

#[derive(Debug, Clone)]
pub struct UniStructDef {
    pub struct_comments: String,
    pub struct_name: String,
    pub struct_fields: Vec<StructField>,
}

#[derive(Debug, Clone)]
pub struct VariantCase {
    pub vc_comments: String,
    pub vc_case_name: String,
    pub vc_case_type: Option<UniDataType>,
}

#[derive(Debug, Clone)]
pub struct UniVariantDef {
    pub variant_comments: String,
    pub variant_name: String,
    pub variant_cases: Vec<VariantCase>,
}

#[derive(Debug, Clone)]
pub struct UniEnumDef {
    pub enum_comments: String,
    pub enum_name: String,
    pub enum_cases: Vec<EnumCase>,
}

#[derive(Debug, Clone)]
pub struct EnumCase {
    pub ec_comments: String,
    pub ec_name: String,
    pub ec_number: u32,
}

impl Display for UniStructDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UniStructDef: {:?}", self)
    }
}

impl Display for UniClassDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UniClassDef: {:?}", self)
    }
}
