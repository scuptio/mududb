//! Identifier types used across the workspace.

/// unique object id
pub type OID = u128;

/// Index of an attribute within a data tuple.
pub type AttrIndex = usize;

/// Position of a datum inside a key or value tuple.
pub type DatumIndex = usize;

/// Identifier of a tuple.
pub type TupleID = u64;

/// Identifier of a worker thread.
pub type ThdID = u64;

/// Reserved sentinel for an invalid object identifier.
pub const INVALID_OID: OID = 0;

/// Truncates an object id to its low 32 bits.
pub fn oid_cast_to_u32(n: u128) -> u32 {
    n as u32
}
