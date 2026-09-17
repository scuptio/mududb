use crate::universal::mp_wire::{FromValue, ToValue, Value, WireError};
use crate::universal::uni_data_value::UniDataValue;

#[derive(Debug, Clone, Default)]
pub struct UniGetResult {
    pub value: Option<UniDataValue>,
}

impl ToValue for UniGetResult {
    fn to_value(&self) -> Value {
        Value::Map(vec![(
            Value::from(1u32),
            crate::universal::mp_wire::to_option(&self.value, |item| item.to_value()),
        )])
    }
}

impl FromValue for UniGetResult {
    fn from_value(value: &Value) -> Result<Self, WireError> {
        let pairs = value.as_map()?;
        let mut value_ = None;
        for (key, val) in pairs {
            let Ok(number) = key.as_u64() else {
                continue;
            };
            if number == 1 {
                value_ = Some(crate::universal::mp_wire::from_option(
                    val,
                    UniDataValue::from_value,
                )?);
            }
        }
        Ok(Self {
            value: value_.unwrap_or_default(),
        })
    }
}
