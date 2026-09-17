use crate::universal::mp_wire::{FromValue, ToValue, Value, WireError};
use crate::universal::uni_oid::UniOid;

#[derive(Debug, Clone, Default)]
pub struct UniSessionOpenArgv {
    pub worker_id: UniOid,
}

impl ToValue for UniSessionOpenArgv {
    fn to_value(&self) -> Value {
        Value::Map(vec![(Value::from(1u32), self.worker_id.to_value())])
    }
}

impl FromValue for UniSessionOpenArgv {
    fn from_value(value: &Value) -> Result<Self, WireError> {
        let pairs = value.as_map()?;
        let mut worker_id = None;
        for (key, val) in pairs {
            let Ok(number) = key.as_u64() else {
                continue;
            };
            if number == 1 {
                worker_id = Some(UniOid::from_value(val)?);
            }
        }
        Ok(Self {
            worker_id: worker_id.unwrap_or_default(),
        })
    }
}
