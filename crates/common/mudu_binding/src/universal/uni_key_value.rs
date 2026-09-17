use crate::universal::mp_wire::{FromValue, ToValue, Value, WireError};
use crate::universal::uni_data_value::UniDataValue;

#[derive(Debug, Clone, Default)]
pub struct UniKeyValue {
    pub key: UniDataValue,

    pub value: UniDataValue,
}

impl ToValue for UniKeyValue {
    fn to_value(&self) -> Value {
        Value::Map(vec![
            (Value::from(1u32), self.key.to_value()),
            (Value::from(2u32), self.value.to_value()),
        ])
    }
}

impl FromValue for UniKeyValue {
    fn from_value(value: &Value) -> Result<Self, WireError> {
        let pairs = value.as_map()?;
        let mut key = None;
        let mut value_ = None;
        for (key_, val) in pairs {
            let Ok(number) = key_.as_u64() else {
                continue;
            };
            match number {
                1 => key = Some(UniDataValue::from_value(val)?),
                2 => value_ = Some(UniDataValue::from_value(val)?),
                _ => {}
            }
        }
        Ok(Self {
            key: key.unwrap_or_default(),
            value: value_.unwrap_or_default(),
        })
    }
}
