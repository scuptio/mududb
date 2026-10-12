use crate::data_bin::DataBin;
use std::ops::Bound;

/// A comparison operator binding a column to a binary-encoded operand.
#[derive(Clone, Debug)]
pub enum Operator {
    /// Column value equals the operand.
    Equal(DataBin),
    /// Column value differs from the operand.
    NonEqual(DataBin),
    /// Column value is greater than the operand.
    Greater(DataBin),
    /// Column value is less than the operand.
    Less(DataBin),
    /// Column value is less than or equal to the operand.
    LessEqual(DataBin),
    /// Column value is greater than or equal to the operand.
    GreaterEqual(DataBin),
    /// Column value falls within the given bounds.
    Range(Bound<DataBin>, Bound<DataBin>),
}
