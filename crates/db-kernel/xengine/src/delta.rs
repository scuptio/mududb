//! Restricted delta-assignment types (`SET col = col <+|-> literal`).

use crate::data_bin::DataBin;
use mudu::common::id::AttrIndex;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

/// Sign of a restricted `SET col = col <+|-> <literal>` update assignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeltaOp {
    /// `col = col + literal`
    Add,
    /// `col = col - literal`
    Sub,
    /// `col = col + literal`, evaluated atomically at COMMIT APPLY time
    /// instead of under the statement lock at statement time. Only valid for
    /// assignments that commute with every other concurrent writer of the
    /// row; see `OptUpdate::delta_assignments`.
    AddDeferred,
    /// `col = col - literal`, deferred like [`DeltaOp::AddDeferred`].
    SubDeferred,
    /// Deferred conditional restock: `col = col - q; if col < floor { col + wrap }`.
    /// The literal packs three big-endian i64s `[q, floor, wrap]`. Two such
    /// updates always commute when `wrap > floor` (the result is
    /// `((current - floor - q) mod wrap) + floor`, order-invariant), so they
    /// are evaluated at COMMIT APPLY time without any statement lock.
    SubWrapDeferred,
}

impl DeltaOp {
    /// Whether this assignment is evaluated at COMMIT APPLY time (lock-free)
    /// rather than at statement time under the statement lock.
    pub fn is_deferred(self) -> bool {
        matches!(
            self,
            DeltaOp::AddDeferred | DeltaOp::SubDeferred | DeltaOp::SubWrapDeferred
        )
    }

    /// Wire op code used in the relation-update payload and `XLUpdate::delta`.
    pub fn op_code(self) -> u8 {
        match self {
            DeltaOp::Add => 0,
            DeltaOp::Sub => 1,
            DeltaOp::AddDeferred => 2,
            DeltaOp::SubDeferred => 3,
            DeltaOp::SubWrapDeferred => 4,
        }
    }

    /// Decodes a wire op code back into a [`DeltaOp`].
    pub fn from_op_code(op: u8) -> RS<DeltaOp> {
        match op {
            0 => Ok(DeltaOp::Add),
            1 => Ok(DeltaOp::Sub),
            2 => Ok(DeltaOp::AddDeferred),
            3 => Ok(DeltaOp::SubDeferred),
            4 => Ok(DeltaOp::SubWrapDeferred),
            other => Err(mudu_error!(
                ErrorCode::Decode,
                format!("unknown relation delta op {other}")
            )),
        }
    }
}

/// A restricted expression assignment (`SET col = col <+|-> <integer literal
/// or ?>`).
///
/// Unlike an absolute assignment carried by [`VecDatum`], the new column value
/// is computed from the latest committed value read under the statement lock,
/// which makes the update an atomic increment/decrement.
#[derive(Clone, Debug)]
pub struct DeltaAssign {
    /// Target column attribute index.
    pub attr: AttrIndex,
    /// Whether to add or subtract the operand.
    pub op: DeltaOp,
    /// Operand encoded in the column's binary format.
    pub literal: DataBin,
}
