use crate::x_engine::dat_bin::DatBin;
use std::ops::Bound;

#[derive(Clone, Debug)]
pub enum Operator {
    Equal(DatBin),
    NonEqual(DatBin),
    Greater(DatBin),
    Less(DatBin),
    LessEqual(DatBin),
    GreaterEqual(DatBin),
    Range(Bound<DatBin>, Bound<DatBin>),
}
