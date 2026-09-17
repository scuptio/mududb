use crate::universal::mp_wire::{FromValue, ToValue, Value, WireError};
use crate::universal::uni_data_value::UniDataValue;
use crate::universal::uni_oid::UniOid;

#[derive(Debug, Clone, Default)]
pub struct UniRangeArgv {
    pub oid: UniOid,

    pub start_key: UniDataValue,

    pub end_key: UniDataValue,
}

impl ToValue for UniRangeArgv {
    fn to_value(&self) -> Value {
        Value::Map(vec![
            (Value::from(1u32), self.oid.to_value()),
            (Value::from(2u32), self.start_key.to_value()),
            (Value::from(3u32), self.end_key.to_value()),
        ])
    }
}

impl FromValue for UniRangeArgv {
    fn from_value(value: &Value) -> Result<Self, WireError> {
        let pairs = value.as_map()?;
        let mut oid = None;
        let mut start_key = None;
        let mut end_key = None;
        for (key, val) in pairs {
            let Ok(number) = key.as_u64() else {
                continue;
            };
            match number {
                1 => oid = Some(UniOid::from_value(val)?),
                2 => start_key = Some(UniDataValue::from_value(val)?),
                3 => end_key = Some(UniDataValue::from_value(val)?),
                _ => {}
            }
        }
        Ok(Self {
            oid: oid.unwrap_or_default(),
            start_key: start_key.unwrap_or_default(),
            end_key: end_key.unwrap_or_default(),
        })
    }
}
