use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use std::any::Any;
use std::fmt::Debug;

/// Object-safe view of a type parameter object: cloning, JSON
/// serialization, and naming.
pub trait DataTypeParamDyn: Debug + Any + Send + Sync {
    /// Returns a boxed clone of this parameter object.
    fn clone_boxed(&self) -> Box<dyn DataTypeParamDyn>;

    /// Overwrites this parameter object with its JSON deserialization.
    fn de_from_json(&mut self, json: &str) -> RS<()>;

    /// Serializes this parameter object to JSON.
    fn se_to_json(&self) -> RS<String>;

    /// Returns the SQL display name of the parameterized type.
    fn name(&self) -> String;
}

/// A statically typed type parameter: a [`DataTypeParamDyn`] with a total
/// order.
pub trait DTPStatic: DataTypeParamDyn + Order + 'static {}
