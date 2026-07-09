use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::error;
use crate::exports::mududb::component_shim::{system, types};
use crate::value;
use mududb::types::datum::DatumDyn;

#[derive(Clone, Default)]
pub struct ValueList {
    named: RefCell<BTreeMap<String, types::Value>>,
    indexed: RefCell<BTreeMap<i32, types::Value>>,
}

impl ValueList {
    pub fn to_facade_values(&self) -> Result<Vec<Box<dyn DatumDyn>>, types::Error> {
        if !self.named.borrow().is_empty() {
            return Err(error::unsupported(
                "named values are not supported by the current mududb facade SQL adapter",
            ));
        }

        let indexed = self.indexed.borrow();
        indexed
            .iter()
            .map(|(index, value)| {
                if *index < 0 {
                    return Err(error::range("value-list index cannot be negative"));
                }
                Ok(Box::new(value::into_data_value(value.clone())) as Box<dyn DatumDyn>)
            })
            .collect()
    }
}

impl system::GuestValueList for ValueList {
    fn new() -> Self {
        Self::default()
    }

    fn bind_named_value(&self, name: String, value: types::Value) {
        self.named.borrow_mut().insert(name, value);
    }

    fn bind_value(&self, index: i32, value: types::Value) {
        self.indexed.borrow_mut().insert(index, value);
    }

    fn len(&self) -> u32 {
        self.indexed.borrow().len() as u32
    }

    fn value(&self, index: u32) -> Result<types::Value, types::Error> {
        self.indexed
            .borrow()
            .get(&(index as i32))
            .cloned()
            .ok_or_else(|| error::range(format!("value-list index {index} is out of range")))
    }
}
