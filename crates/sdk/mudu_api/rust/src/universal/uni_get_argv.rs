use crate::universal::mp_wire::{FromValue, ToValue, Value, WireError};
use crate::universal::uni_data_value::UniDataValue;
use crate::universal::uni_oid::UniOid;

#[derive(Debug, Clone, Default)]
pub struct UniGetArgv {
    pub oid: UniOid,

    pub key: UniDataValue,
}

impl ToValue for UniGetArgv {
    fn to_value(&self) -> Value {
        Value::Map(vec![
            (Value::from(1u32), self.oid.to_value()),
            (Value::from(2u32), self.key.to_value()),
        ])
    }
}

impl FromValue for UniGetArgv {
    fn from_value(value: &Value) -> Result<Self, WireError> {
        let pairs = value.as_map()?;
        let mut oid = None;
        let mut key = None;
        for (key_, val) in pairs {
            let Ok(number) = key_.as_u64() else {
                continue;
            };
            match number {
                1 => oid = Some(UniOid::from_value(val)?),
                2 => key = Some(UniDataValue::from_value(val)?),
                _ => {}
            }
        }
        Ok(Self {
            oid: oid.unwrap_or_default(),
            key: key.unwrap_or_default(),
        })
    }
}
