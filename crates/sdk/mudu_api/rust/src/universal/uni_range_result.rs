use crate::universal::mp_wire::{FromValue, ToValue, Value, WireError};
use crate::universal::uni_key_value::UniKeyValue;

#[derive(Debug, Clone, Default)]
pub struct UniRangeResult {
    pub items: Vec<UniKeyValue>,
}

impl ToValue for UniRangeResult {
    fn to_value(&self) -> Value {
        Value::Map(vec![(
            Value::from(1u32),
            crate::universal::mp_wire::to_array(&self.items, |item| item.to_value()),
        )])
    }
}

impl FromValue for UniRangeResult {
    fn from_value(value: &Value) -> Result<Self, WireError> {
        let pairs = value.as_map()?;
        let mut items = None;
        for (key, val) in pairs {
            let Ok(number) = key.as_u64() else {
                continue;
            };
            if number == 1 {
                items = Some(crate::universal::mp_wire::from_array(
                    val,
                    UniKeyValue::from_value,
                )?);
            }
        }
        Ok(Self {
            items: items.unwrap_or_default(),
        })
    }
}
