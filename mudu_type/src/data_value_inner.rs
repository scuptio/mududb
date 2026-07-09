use crate::datum::DatumDyn;

pub trait DataValueInner: DatumDyn + 'static {}
