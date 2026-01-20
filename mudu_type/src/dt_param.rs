use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use std::any::Any;
use std::fmt::Debug;

pub trait DTPDyn: Debug + Any + Send + Sync {
    fn clone_boxed(&self) -> Box<dyn DTPDyn>;

    fn de_from_json(&mut self, json: &str) -> RS<()>;

    fn se_to_json(&self) -> RS<String>;

    fn name(&self) -> String;
}

pub trait DTPStatic: DTPDyn + Order + 'static {}
