use crate::x_engine::data_bin::DataBin;
use std::ops::Bound;

#[derive(Clone, Debug)]
pub enum Operator {
    Equal(DataBin),
    NonEqual(DataBin),
    Greater(DataBin),
    Less(DataBin),
    LessEqual(DataBin),
    GreaterEqual(DataBin),
    Range(Bound<DataBin>, Bound<DataBin>),
}
