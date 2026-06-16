use crate::universal::uni_dat_type::UniDatType;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[derive(Default)]
pub struct UniResultType {
    pub ok: Option<Box<UniDatType>>,

    pub err: Option<Box<UniDatType>>,
}

