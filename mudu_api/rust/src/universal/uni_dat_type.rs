use crate::universal::uni_scalar::UniScalar;

use crate::universal::uni_record_type::UniRecordType;

use crate::universal::uni_result_type::UniResultType;

#[derive(Debug, Clone)]

pub enum UniDatType {
    Scalar(UniScalar),

    Array(Box<UniDatType>),

    Record(UniRecordType),

    Option(Box<UniDatType>),

    Tuple(Vec<UniDatType>),

    Result(UniResultType),

    Box(Box<UniDatType>),

    Identifier(String),

    Binary,
}

impl Default for UniDatType {
    fn default() -> Self {
        Self::Scalar(Default::default())
    }
}

impl UniDatType {
    pub fn from_scalar(inner: UniScalar) -> Self {
        Self::Scalar(inner)
    }

    pub fn as_scalar(&self) -> Option<&UniScalar> {
        match self {
            Self::Scalar(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_scalar(&self) -> &UniScalar {
        match self {
            Self::Scalar(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_array(inner: Box<UniDatType>) -> Self {
        Self::Array(inner)
    }

    pub fn as_array(&self) -> Option<&UniDatType> {
        match self {
            Self::Array(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_array(&self) -> &UniDatType {
        match self {
            Self::Array(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_record(inner: UniRecordType) -> Self {
        Self::Record(inner)
    }

    pub fn as_record(&self) -> Option<&UniRecordType> {
        match self {
            Self::Record(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_record(&self) -> &UniRecordType {
        match self {
            Self::Record(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_option(inner: Box<UniDatType>) -> Self {
        Self::Option(inner)
    }

    pub fn as_option(&self) -> Option<&UniDatType> {
        match self {
            Self::Option(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_option(&self) -> &UniDatType {
        match self {
            Self::Option(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_tuple(inner: Vec<UniDatType>) -> Self {
        Self::Tuple(inner)
    }

    pub fn as_tuple(&self) -> Option<&Vec<UniDatType>> {
        match self {
            Self::Tuple(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_tuple(&self) -> &Vec<UniDatType> {
        match self {
            Self::Tuple(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_result(inner: UniResultType) -> Self {
        Self::Result(inner)
    }

    pub fn as_result(&self) -> Option<&UniResultType> {
        match self {
            Self::Result(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_result(&self) -> &UniResultType {
        match self {
            Self::Result(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_box(inner: Box<UniDatType>) -> Self {
        Self::Box(inner)
    }

    pub fn as_box(&self) -> Option<&UniDatType> {
        match self {
            Self::Box(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_box(&self) -> &UniDatType {
        match self {
            Self::Box(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_identifier(inner: String) -> Self {
        Self::Identifier(inner)
    }

    pub fn as_identifier(&self) -> Option<&String> {
        match self {
            Self::Identifier(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_identifier(&self) -> &String {
        match self {
            Self::Identifier(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_binary() -> Self {
        Self::Binary
    }

    pub fn as_binary(&self) -> Option<()> {
        match self {
            Self::Binary => Some(()),
            _ => None,
        }
    }

    pub fn expect_binary(&self) {
        match self {
            Self::Binary => (),
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn is_binary(&self) -> bool {
        matches!(self, Self::Binary)
    }
}

impl serde::Serialize for UniDatType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut serialize_seq = serializer.serialize_seq(Some(2))?;
        match self {
            UniDatType::Scalar(inner) => {
                serialize_seq.serialize_element(&0u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDatType::Array(inner) => {
                serialize_seq.serialize_element(&1u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDatType::Record(inner) => {
                serialize_seq.serialize_element(&2u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDatType::Option(inner) => {
                serialize_seq.serialize_element(&3u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDatType::Tuple(inner) => {
                serialize_seq.serialize_element(&4u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDatType::Result(inner) => {
                serialize_seq.serialize_element(&5u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDatType::Box(inner) => {
                serialize_seq.serialize_element(&6u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDatType::Identifier(inner) => {
                serialize_seq.serialize_element(&7u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDatType::Binary => {
                // has no inner payload, write a dummy u8 value
                serialize_seq.serialize_element(&8u32)?;
                serialize_seq.serialize_element(&0u8)?
            }
        }
        serialize_seq.end()
    }
}

struct UniDatTypeVisitor {}

impl<'de> serde::de::Visitor<'de> for UniDatTypeVisitor {
    type Value = UniDatType;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a sequence")
    }

    fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        use serde::de::Error;
        use serde::de::Unexpected;
        let mut seq = seq;
        let key = seq.next_element::<u32>()?;
        let id = match key {
            Some(key) => key,
            None => {
                return Err(Error::invalid_value(Unexpected::Seq, &self));
            }
        };
        match id {
            0 => {
                let value = seq
                    .next_element::<UniScalar>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Scalar(value))
            }

            1 => {
                let value = seq
                    .next_element::<Box<UniDatType>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Array(value))
            }

            2 => {
                let value = seq
                    .next_element::<UniRecordType>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Record(value))
            }

            3 => {
                let value = seq
                    .next_element::<Box<UniDatType>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Option(value))
            }

            4 => {
                let value = seq
                    .next_element::<Vec<UniDatType>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Tuple(value))
            }

            5 => {
                let value = seq
                    .next_element::<UniResultType>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Result(value))
            }

            6 => {
                let value = seq
                    .next_element::<Box<UniDatType>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Box(value))
            }

            7 => {
                let value = seq
                    .next_element::<String>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Identifier(value))
            }

            8 => {
                // has no inner payload, consume a dummy u8 value
                let _ = seq
                    .next_element::<u8>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Binary)
            }

            _ => Err(Error::invalid_value(Unexpected::Map, &self)),
        }
    }
}

impl<'de> serde::Deserialize<'de> for UniDatType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(UniDatTypeVisitor {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universal::uni_record_type::{UniRecordField, UniRecordType};
    use crate::universal::uni_result_type::UniResultType;
    use crate::universal::uni_scalar::UniScalar;
    use mudu::common::serde_utils::{
        deserialize_from, deserialize_from_json, serialize_to_json, serialize_to_vec,
    };

    fn assert_json_and_binary_roundtrip(value: &UniDatType) {
        let json = serialize_to_json(value).unwrap();
        let binary = serialize_to_vec(value).unwrap();

        let decoded_json: UniDatType = deserialize_from_json(json.as_str()).unwrap();
        let (decoded_binary, used): (UniDatType, u64) =
            deserialize_from(binary.as_slice()).unwrap();

        let json_after = serialize_to_json(&decoded_json).unwrap();
        let binary_after = serialize_to_vec(&decoded_binary).unwrap();

        assert_eq!(json_after, json);
        assert_eq!(binary_after, binary);
        assert_eq!(used as usize, binary.len());
    }

    fn sample_record_type() -> UniRecordType {
        UniRecordType {
            record_name: "vote_record".to_string(),
            record_fields: vec![
                UniRecordField {
                    field_name: "id".to_string(),
                    field_type: UniDatType::Scalar(UniScalar::U128),
                },
                UniRecordField {
                    field_name: "name".to_string(),
                    field_type: UniDatType::Scalar(UniScalar::String),
                },
                UniRecordField {
                    field_name: "tags".to_string(),
                    field_type: UniDatType::Array(Box::new(UniDatType::Scalar(UniScalar::String))),
                },
            ],
        }
    }

    fn sample_dat_type() -> UniDatType {
        UniDatType::Record(UniRecordType {
            record_name: "envelope".to_string(),
            record_fields: vec![
                UniRecordField {
                    field_name: "meta".to_string(),
                    field_type: UniDatType::Tuple(vec![
                        UniDatType::Scalar(UniScalar::U64),
                        UniDatType::Option(Box::new(UniDatType::Scalar(UniScalar::String))),
                    ]),
                },
                UniRecordField {
                    field_name: "payload".to_string(),
                    field_type: UniDatType::Result(UniResultType {
                        ok: Some(Box::new(UniDatType::Array(Box::new(UniDatType::Scalar(
                            UniScalar::I32,
                        ))))),
                        err: Some(Box::new(UniDatType::Identifier("ErrCode".to_string()))),
                    }),
                },
                UniRecordField {
                    field_name: "blob".to_string(),
                    field_type: UniDatType::Binary,
                },
            ],
        })
    }

    #[test]
    fn default_is_scalar_bool() {
        assert!(matches!(
            UniDatType::default(),
            UniDatType::Scalar(UniScalar::Bool)
        ));
    }

    #[test]
    fn constructors_accessors_and_expects() {
        let scalar = UniDatType::from_scalar(UniScalar::I32);
        assert_eq!(scalar.as_scalar(), Some(&UniScalar::I32));
        assert!(scalar.as_array().is_none());
        assert_eq!(scalar.expect_scalar(), &UniScalar::I32);

        let array = UniDatType::from_array(Box::new(UniDatType::Scalar(UniScalar::String)));
        assert!(matches!(
            array.as_array(),
            Some(UniDatType::Scalar(UniScalar::String))
        ));
        assert!(array.as_scalar().is_none());
        assert!(matches!(
            array.expect_array(),
            UniDatType::Scalar(UniScalar::String)
        ));

        let record = UniDatType::from_record(sample_record_type());
        assert!(record.as_record().is_some());
        assert!(record.as_scalar().is_none());
        let inner = record.expect_record();
        assert_eq!(inner.record_name, "vote_record");

        let option = UniDatType::from_option(Box::new(UniDatType::Scalar(UniScalar::I64)));
        assert!(matches!(
            option.as_option(),
            Some(UniDatType::Scalar(UniScalar::I64))
        ));
        assert!(option.as_scalar().is_none());
        assert!(matches!(
            option.expect_option(),
            UniDatType::Scalar(UniScalar::I64)
        ));

        let tuple = UniDatType::from_tuple(vec![
            UniDatType::Scalar(UniScalar::I32),
            UniDatType::Scalar(UniScalar::String),
        ]);
        let tuple_inner = tuple.as_tuple().expect("tuple");
        assert_eq!(tuple_inner.len(), 2);
        assert!(matches!(tuple_inner[0], UniDatType::Scalar(UniScalar::I32)));
        assert!(matches!(
            tuple_inner[1],
            UniDatType::Scalar(UniScalar::String)
        ));
        assert!(tuple.as_scalar().is_none());
        let expect_inner = tuple.expect_tuple();
        assert_eq!(expect_inner.len(), 2);
        assert!(matches!(
            expect_inner[0],
            UniDatType::Scalar(UniScalar::I32)
        ));
        assert!(matches!(
            expect_inner[1],
            UniDatType::Scalar(UniScalar::String)
        ));

        let result = UniDatType::from_result(UniResultType {
            ok: Some(Box::new(UniDatType::Scalar(UniScalar::I32))),
            err: Some(Box::new(UniDatType::Scalar(UniScalar::String))),
        });
        assert!(result.as_result().is_some());
        assert!(result.as_scalar().is_none());
        let inner = result.expect_result();
        assert!(inner.ok.is_some());
        assert!(inner.err.is_some());

        let bx = UniDatType::from_box(Box::new(UniDatType::Scalar(UniScalar::F64)));
        assert!(matches!(
            bx.as_box(),
            Some(UniDatType::Scalar(UniScalar::F64))
        ));
        assert!(bx.as_scalar().is_none());
        assert!(matches!(
            bx.expect_box(),
            UniDatType::Scalar(UniScalar::F64)
        ));

        let identifier = UniDatType::from_identifier("MyType".to_string());
        assert_eq!(identifier.as_identifier(), Some(&"MyType".to_string()));
        assert!(identifier.as_scalar().is_none());
        assert_eq!(identifier.expect_identifier(), "MyType");

        let binary = UniDatType::from_binary();
        assert!(binary.is_binary());
        assert_eq!(binary.as_binary(), Some(()));
        assert!(binary.as_scalar().is_none());
        binary.expect_binary();
    }

    #[test]
    fn serde_roundtrip_for_variants() {
        assert_json_and_binary_roundtrip(&UniDatType::Scalar(UniScalar::I32));
        assert_json_and_binary_roundtrip(&UniDatType::Array(Box::new(UniDatType::Scalar(
            UniScalar::String,
        ))));
        assert_json_and_binary_roundtrip(&UniDatType::Record(sample_record_type()));
        assert_json_and_binary_roundtrip(&UniDatType::Option(Box::new(UniDatType::Scalar(
            UniScalar::I64,
        ))));
        assert_json_and_binary_roundtrip(&UniDatType::Tuple(vec![
            UniDatType::Scalar(UniScalar::I32),
            UniDatType::Scalar(UniScalar::String),
        ]));
        assert_json_and_binary_roundtrip(&UniDatType::Result(UniResultType {
            ok: Some(Box::new(UniDatType::Scalar(UniScalar::I32))),
            err: Some(Box::new(UniDatType::Identifier("err".to_string()))),
        }));
        assert_json_and_binary_roundtrip(&UniDatType::Box(Box::new(UniDatType::Scalar(
            UniScalar::F64,
        ))));
        assert_json_and_binary_roundtrip(&UniDatType::Identifier("MyType".to_string()));
        assert_json_and_binary_roundtrip(&UniDatType::Binary);
    }

    #[test]
    fn deserialize_rejects_invalid_and_truncated_tags() {
        assert!(deserialize_from_json::<UniDatType>("[99,0]").is_err());
        assert!(deserialize_from_json::<UniDatType>("[0]").is_err());
        assert!(deserialize_from_json::<UniDatType>("[8]").is_err());
    }

    #[test]
    fn json_shape_sanity() {
        let scalar_json = serialize_to_json(&UniDatType::Scalar(UniScalar::I32)).unwrap();
        let scalar_compact: String = scalar_json.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(scalar_compact, "[0,6]");

        let binary_json = serialize_to_json(&UniDatType::Binary).unwrap();
        let binary_compact: String = binary_json.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(binary_compact, "[8,0]");

        let decoded: UniDatType = deserialize_from_json("[8,0]").unwrap();
        assert!(decoded.is_binary());
        assert_eq!(decoded.as_binary(), Some(()));
    }

    #[test]
    fn nested_record_roundtrip() {
        assert_json_and_binary_roundtrip(&sample_dat_type());
    }
}
