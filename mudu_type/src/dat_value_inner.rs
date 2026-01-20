use crate::datum::DatumDyn;

pub trait DatValueInner: DatumDyn + 'static {}
