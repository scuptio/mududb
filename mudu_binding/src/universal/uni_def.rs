use crate::universal::uni_dat_type::UniDatType;
use std::fmt;
use std::fmt::{Debug, Display};
#[derive(Debug, Clone)]
pub struct RecordField {
    pub rf_comments: String,
    pub rf_name: String,
    pub rf_type: UniDatType,
}

#[derive(Debug, Clone)]
pub struct UniRecordDef {
    pub record_comments: String,
    pub record_name: String,
    pub record_fields: Vec<RecordField>,
}

#[derive(Debug, Clone)]
pub struct VariantCase {
    pub vc_comments: String,
    pub vc_case_name: String,
    pub vc_case_type: Option<UniDatType>,
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

impl Display for UniRecordDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RecordDef: {:?}", self)
    }
}

impl UniRecordDef {}
