use crate::universal::uni_scalar::UniScalar;

use crate::universal::uni_record_type::UniRecordType;

use crate::universal::uni_result_type::UniResultType;

#[derive(Debug, Clone)]

pub enum UniDataType {
    Scalar(UniScalar),

    Array(Box<UniDataType>),

    Record(UniRecordType),

    Option(Box<UniDataType>),

    Tuple(Vec<UniDataType>),

    Result(UniResultType),

    Identifier(String),

    Box(Box<UniDataType>),

    Binary,
}

impl Default for UniDataType {
    fn default() -> Self {
        Self::Scalar(Default::default())
    }
}

impl UniDataType {
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

    pub fn from_array(inner: Box<UniDataType>) -> Self {
        Self::Array(inner)
    }

    pub fn as_array(&self) -> Option<&UniDataType> {
        match self {
            Self::Array(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_array(&self) -> &UniDataType {
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

    pub fn from_option(inner: Box<UniDataType>) -> Self {
        Self::Option(inner)
    }

    pub fn as_option(&self) -> Option<&UniDataType> {
        match self {
            Self::Option(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_option(&self) -> &UniDataType {
        match self {
            Self::Option(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_tuple(inner: Vec<UniDataType>) -> Self {
        Self::Tuple(inner)
    }

    pub fn as_tuple(&self) -> Option<&Vec<UniDataType>> {
        match self {
            Self::Tuple(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_tuple(&self) -> &Vec<UniDataType> {
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

    pub fn from_box(inner: Box<UniDataType>) -> Self {
        Self::Box(inner)
    }

    pub fn as_box(&self) -> Option<&UniDataType> {
        match self {
            Self::Box(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_box(&self) -> &UniDataType {
        match self {
            Self::Box(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn is_box(&self) -> bool {
        matches!(self, Self::Box(_))
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

impl serde::Serialize for UniDataType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut serialize_seq = serializer.serialize_seq(Some(2))?;
        match self {
            UniDataType::Scalar(inner) => {
                serialize_seq.serialize_element(&0u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataType::Array(inner) => {
                serialize_seq.serialize_element(&1u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataType::Record(inner) => {
                serialize_seq.serialize_element(&2u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataType::Option(inner) => {
                serialize_seq.serialize_element(&3u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataType::Tuple(inner) => {
                serialize_seq.serialize_element(&4u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataType::Result(inner) => {
                serialize_seq.serialize_element(&5u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataType::Identifier(inner) => {
                serialize_seq.serialize_element(&6u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataType::Binary => {
                // has no inner payload, write a dummy u8 value
                serialize_seq.serialize_element(&7u32)?;
                serialize_seq.serialize_element(&0u8)?
            }

            UniDataType::Box(inner) => {
                serialize_seq.serialize_element(&8u32)?;
                serialize_seq.serialize_element(&inner)?;
            }
        }
        serialize_seq.end()
    }
}

struct UniDataTypeVisitor {}

impl<'de> serde::de::Visitor<'de> for UniDataTypeVisitor {
    type Value = UniDataType;

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
                    .next_element::<Box<UniDataType>>()?
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
                    .next_element::<Box<UniDataType>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Option(value))
            }

            4 => {
                let value = seq
                    .next_element::<Vec<UniDataType>>()?
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
                    .next_element::<String>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Identifier(value))
            }

            7 => {
                // has no inner payload, consume a dummy u8 value
                let _ = seq
                    .next_element::<u8>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Binary)
            }

            8 => {
                let value = seq
                    .next_element::<Box<UniDataType>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Box(value))
            }

            _ => Err(Error::invalid_value(Unexpected::Map, &self)),
        }
    }
}

impl<'de> serde::Deserialize<'de> for UniDataType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(UniDataTypeVisitor {})
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

    fn assert_json_and_binary_roundtrip(value: &UniDataType) {
        let json = serialize_to_json(value).unwrap();
        let binary = serialize_to_vec(value).unwrap();

        let decoded_json: UniDataType = deserialize_from_json(json.as_str()).unwrap();
        let (decoded_binary, used): (UniDataType, u64) =
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
                    field_type: UniDataType::Scalar(UniScalar::U128),
                    field_attrs: Vec::new(),
                },
                UniRecordField {
                    field_name: "name".to_string(),
                    field_type: UniDataType::Scalar(UniScalar::String),
                    field_attrs: Vec::new(),
                },
                UniRecordField {
                    field_name: "tags".to_string(),
                    field_type: UniDataType::Array(Box::new(UniDataType::Scalar(
                        UniScalar::String,
                    ))),
                    field_attrs: Vec::new(),
                },
            ],
        }
    }

    fn sample_data_type() -> UniDataType {
        UniDataType::Record(UniRecordType {
            record_name: "envelope".to_string(),
            record_fields: vec![
                UniRecordField {
                    field_name: "meta".to_string(),
                    field_type: UniDataType::Tuple(vec![
                        UniDataType::Scalar(UniScalar::U64),
                        UniDataType::Option(Box::new(UniDataType::Scalar(UniScalar::String))),
                    ]),
                    field_attrs: Vec::new(),
                },
                UniRecordField {
                    field_name: "payload".to_string(),
                    field_type: UniDataType::Result(UniResultType {
                        ok: Some(Box::new(UniDataType::Array(Box::new(UniDataType::Scalar(
                            UniScalar::I32,
                        ))))),
                        err: Some(Box::new(UniDataType::Identifier("ErrCode".to_string()))),
                    }),
                    field_attrs: Vec::new(),
                },
                UniRecordField {
                    field_name: "blob".to_string(),
                    field_type: UniDataType::Binary,
                    field_attrs: Vec::new(),
                },
            ],
        })
    }

    #[test]
    fn default_is_scalar_bool() {
        assert!(matches!(
            UniDataType::default(),
            UniDataType::Scalar(UniScalar::Bool)
        ));
    }

    #[test]
    fn constructors_accessors_and_expects() {
        let scalar = UniDataType::from_scalar(UniScalar::I32);
        assert_eq!(scalar.as_scalar(), Some(&UniScalar::I32));
        assert!(scalar.as_array().is_none());
        assert_eq!(scalar.expect_scalar(), &UniScalar::I32);

        let array = UniDataType::from_array(Box::new(UniDataType::Scalar(UniScalar::String)));
        assert!(matches!(
            array.as_array(),
            Some(UniDataType::Scalar(UniScalar::String))
        ));
        assert!(array.as_scalar().is_none());
        assert!(matches!(
            array.expect_array(),
            UniDataType::Scalar(UniScalar::String)
        ));

        let record = UniDataType::from_record(sample_record_type());
        assert!(record.as_record().is_some());
        assert!(record.as_scalar().is_none());
        let inner = record.expect_record();
        assert_eq!(inner.record_name, "vote_record");

        let option = UniDataType::from_option(Box::new(UniDataType::Scalar(UniScalar::I64)));
        assert!(matches!(
            option.as_option(),
            Some(UniDataType::Scalar(UniScalar::I64))
        ));
        assert!(option.as_scalar().is_none());
        assert!(matches!(
            option.expect_option(),
            UniDataType::Scalar(UniScalar::I64)
        ));

        let tuple = UniDataType::from_tuple(vec![
            UniDataType::Scalar(UniScalar::I32),
            UniDataType::Scalar(UniScalar::String),
        ]);
        let tuple_inner = tuple.as_tuple().expect("tuple");
        assert_eq!(tuple_inner.len(), 2);
        assert!(matches!(
            tuple_inner[0],
            UniDataType::Scalar(UniScalar::I32)
        ));
        assert!(matches!(
            tuple_inner[1],
            UniDataType::Scalar(UniScalar::String)
        ));
        assert!(tuple.as_scalar().is_none());
        let expect_inner = tuple.expect_tuple();
        assert_eq!(expect_inner.len(), 2);
        assert!(matches!(
            expect_inner[0],
            UniDataType::Scalar(UniScalar::I32)
        ));
        assert!(matches!(
            expect_inner[1],
            UniDataType::Scalar(UniScalar::String)
        ));

        let result = UniDataType::from_result(UniResultType {
            ok: Some(Box::new(UniDataType::Scalar(UniScalar::I32))),
            err: Some(Box::new(UniDataType::Scalar(UniScalar::String))),
        });
        assert!(result.as_result().is_some());
        assert!(result.as_scalar().is_none());
        let inner = result.expect_result();
        assert!(inner.ok.is_some());
        assert!(inner.err.is_some());

        let identifier = UniDataType::from_identifier("MyType".to_string());
        assert_eq!(identifier.as_identifier(), Some(&"MyType".to_string()));
        assert!(identifier.as_scalar().is_none());
        assert_eq!(identifier.expect_identifier(), "MyType");

        let binary = UniDataType::from_binary();
        assert!(binary.is_binary());
        assert_eq!(binary.as_binary(), Some(()));
        assert!(binary.as_scalar().is_none());
        binary.expect_binary();
    }

    #[test]
    fn serde_roundtrip_for_variants() {
        assert_json_and_binary_roundtrip(&UniDataType::Scalar(UniScalar::I32));
        assert_json_and_binary_roundtrip(&UniDataType::Array(Box::new(UniDataType::Scalar(
            UniScalar::String,
        ))));
        assert_json_and_binary_roundtrip(&UniDataType::Record(sample_record_type()));
        assert_json_and_binary_roundtrip(&UniDataType::Option(Box::new(UniDataType::Scalar(
            UniScalar::I64,
        ))));
        assert_json_and_binary_roundtrip(&UniDataType::Tuple(vec![
            UniDataType::Scalar(UniScalar::I32),
            UniDataType::Scalar(UniScalar::String),
        ]));
        assert_json_and_binary_roundtrip(&UniDataType::Result(UniResultType {
            ok: Some(Box::new(UniDataType::Scalar(UniScalar::I32))),
            err: Some(Box::new(UniDataType::Identifier("err".to_string()))),
        }));
        assert_json_and_binary_roundtrip(&UniDataType::Identifier("MyType".to_string()));
        assert_json_and_binary_roundtrip(&UniDataType::Binary);
        assert_json_and_binary_roundtrip(&UniDataType::Box(Box::new(UniDataType::Scalar(
            UniScalar::I32,
        ))));
    }

    #[test]
    fn deserialize_rejects_invalid_and_truncated_tags() {
        assert!(deserialize_from_json::<UniDataType>("[99,0]").is_err());
        assert!(deserialize_from_json::<UniDataType>("[0]").is_err());
        assert!(deserialize_from_json::<UniDataType>("[7]").is_err());
    }

    #[test]
    fn json_shape_sanity() {
        let scalar_json = serialize_to_json(&UniDataType::Scalar(UniScalar::I32)).unwrap();
        let scalar_compact: String = scalar_json.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(scalar_compact, "[0,6]");

        let binary_json = serialize_to_json(&UniDataType::Binary).unwrap();
        let binary_compact: String = binary_json.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(binary_compact, "[7,0]");

        let decoded: UniDataType = deserialize_from_json("[7,0]").unwrap();
        assert!(decoded.is_binary());
        assert_eq!(decoded.as_binary(), Some(()));
    }

    #[test]
    fn nested_record_roundtrip() {
        assert_json_and_binary_roundtrip(&sample_data_type());
    }
}
