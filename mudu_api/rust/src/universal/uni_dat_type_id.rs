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

    OID = 9,

    I128 = 10,

    F32 = 11,

    F64 = 12,

    Char = 13,

    String = 14,

    Array = 15,

    Record = 16,

    Binary = 17,

    Numeric = 18,

    Date = 19,

    Time = 20,

    Timestamp = 21,

    TimestampTz = 22,
}

impl Default for UniDatTypeId {
    fn default() -> Self {
        Self::Bool
    }
}
