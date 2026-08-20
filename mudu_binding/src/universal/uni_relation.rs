//! Relation-level syscall payload element types.
//!
//! The `relation-get` / `relation-update` / `relation-insert` syscalls
//! address one relation row by its primary-key columns. Attributes are
//! addressed by their column index in the original table definition
//! (`AttrIndex` on the kernel side, carried as `u64` on the wire); datums are
//! the column's binary encoding (the same `send`/`recv` binary format used
//! for tuple fields).

/// `relation-update` delta operand sign: `col = col + datum`.
pub const RELATION_DELTA_OP_ADD: u8 = 0;

/// `relation-update` delta operand sign: `col = col - datum`.
pub const RELATION_DELTA_OP_SUB: u8 = 1;

/// Deferred `col = col + datum`: evaluated atomically against the latest
/// committed row at COMMIT APPLY time instead of under the statement lock at
/// statement time. The caller opts into lock-free semantics and must only use
/// it for assignments that commute with every other concurrent writer of the
/// row (plain add/sub qualify).
pub const RELATION_DELTA_OP_ADD_DEFERRED: u8 = 2;

/// Deferred `col = col - datum`; same apply-time/lock-free contract as
/// [`RELATION_DELTA_OP_ADD_DEFERRED`].
pub const RELATION_DELTA_OP_SUB_DEFERRED: u8 = 3;

/// Deferred conditional restock: `col = col - q; if col < floor { col + wrap }`.
/// The datum packs three big-endian i64s `[q, floor, wrap]`. Evaluated
/// atomically at COMMIT APPLY time; two such updates always commute when
/// `wrap > floor` (the result is `((current - floor - q) mod wrap) + floor`,
/// which is order-invariant).
pub const RELATION_DELTA_OP_SUB_WRAP_DEFERRED: u8 = 4;

/// A `SET col = col <+|-> datum` assignment of a `relation-update` call,
/// evaluated against the latest committed row under the statement lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniRelationDelta {
    /// Target column index in the original table definition.
    pub attr: u64,
    /// [`RELATION_DELTA_OP_ADD`] or [`RELATION_DELTA_OP_SUB`].
    pub op: u8,
    /// Operand in the column's binary encoding.
    pub datum: Vec<u8>,
}

impl UniRelationDelta {
    pub fn add(attr: u64, datum: Vec<u8>) -> Self {
        Self {
            attr,
            op: RELATION_DELTA_OP_ADD,
            datum,
        }
    }

    pub fn sub(attr: u64, datum: Vec<u8>) -> Self {
        Self {
            attr,
            op: RELATION_DELTA_OP_SUB,
            datum,
        }
    }

    /// Deferred (apply-time, lock-free) increment, see
    /// [`RELATION_DELTA_OP_ADD_DEFERRED`].
    pub fn add_deferred(attr: u64, datum: Vec<u8>) -> Self {
        Self {
            attr,
            op: RELATION_DELTA_OP_ADD_DEFERRED,
            datum,
        }
    }

    /// Deferred (apply-time, lock-free) decrement, see
    /// [`RELATION_DELTA_OP_SUB_DEFERRED`].
    pub fn sub_deferred(attr: u64, datum: Vec<u8>) -> Self {
        Self {
            attr,
            op: RELATION_DELTA_OP_SUB_DEFERRED,
            datum,
        }
    }

    /// Deferred conditional restock, see
    /// [`RELATION_DELTA_OP_SUB_WRAP_DEFERRED`].
    pub fn sub_wrap(attr: u64, quantity: i64, floor: i64, wrap: i64) -> Self {
        let mut datum = Vec::with_capacity(24);
        datum.extend_from_slice(&quantity.to_be_bytes());
        datum.extend_from_slice(&floor.to_be_bytes());
        datum.extend_from_slice(&wrap.to_be_bytes());
        Self {
            attr,
            op: RELATION_DELTA_OP_SUB_WRAP_DEFERRED,
            datum,
        }
    }
}
