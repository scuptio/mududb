use crate::universal::mp_wire::{FromValue, ToValue, Value, WireError};

#[derive(Debug, Clone, Default)]
pub struct UniPutResult {
    pub ok: bool,
}

impl ToValue for UniPutResult {
    fn to_value(&self) -> Value {
        Value::Map(vec![(Value::from(1u32), self.ok.to_value())])
    }
}

impl FromValue for UniPutResult {
    fn from_value(value: &Value) -> Result<Self, WireError> {
        let pairs = value.as_map()?;
        let mut ok = None;
        for (key, val) in pairs {
            let Ok(number) = key.as_u64() else {
                continue;
            };
            if number == 1 {
                ok = Some(bool::from_value(val)?);
            }
        }
        Ok(Self {
            ok: ok.unwrap_or_default(),
        })
    }
}
