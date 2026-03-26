#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
)]
#[repr(u32)]
pub enum UniDatTypeId {
    Bool = 0,

    U8 = 1,

    I8 = 2,

    U16 = 3,

    I16 = 4,

    U32 = 5,

    I32 = 6,

    U64 = 7,

    I64 = 8,

    F32 = 9,

    F64 = 10,

    Char = 11,

    String = 12,

    Array = 13,

    Record = 14,

    Binary = 15,
}

impl Default for UniDatTypeId {
    fn default() -> Self {
        Self::Bool
    }
}
