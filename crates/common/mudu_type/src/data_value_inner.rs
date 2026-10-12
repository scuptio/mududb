use crate::datum::DatumDyn;

/// Marker trait for types usable as the inner value of a typed datum.
pub trait DataValueInner: DatumDyn + 'static {}
