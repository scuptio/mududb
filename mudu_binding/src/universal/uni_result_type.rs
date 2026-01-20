use crate::universal::uni_dat_type::UniDatType;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniResultType {
    pub ok: Option<Box<UniDatType>>,

    pub err: Option<Box<UniDatType>>,
}

impl Default for UniResultType {
    fn default() -> Self {
        Self {
            ok: Default::default(),

            err: Default::default(),
        }
    }
}
