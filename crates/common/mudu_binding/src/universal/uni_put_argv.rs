use crate::universal::mp_wire::{FromValue, ToValue, Value, WireError};
use crate::universal::uni_data_value::UniDataValue;
use crate::universal::uni_oid::UniOid;

#[derive(Debug, Clone, Default)]
pub struct UniPutArgv {
    pub oid: UniOid,

    pub key: UniDataValue,

    pub value: UniDataValue,
}

impl ToValue for UniPutArgv {
    fn to_value(&self) -> Value {
        Value::Map(vec![
            (Value::from(1u32), self.oid.to_value()),
            (Value::from(2u32), self.key.to_value()),
            (Value::from(3u32), self.value.to_value()),
        ])
    }
}

impl FromValue for UniPutArgv {
    fn from_value(value: &Value) -> Result<Self, WireError> {
        let pairs = value.as_map()?;
        let mut oid = None;
        let mut key = None;
        let mut value_ = None;
        for (key_, val) in pairs {
            let Ok(number) = key_.as_u64() else {
                continue;
            };
            match number {
                1 => oid = Some(UniOid::from_value(val)?),
                2 => key = Some(UniDataValue::from_value(val)?),
                3 => value_ = Some(UniDataValue::from_value(val)?),
                _ => {}
            }
        }
        Ok(Self {
            oid: oid.unwrap_or_default(),
            key: key.unwrap_or_default(),
            value: value_.unwrap_or_default(),
        })
    }
}
