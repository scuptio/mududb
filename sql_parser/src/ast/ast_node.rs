use std::any::Any;
use std::fmt::Debug;

pub trait ASTNode: Any + Debug + Send + Sync {}

pub fn ast_cast_to<T: 'static>(_expr: Box<dyn ASTNode>) -> Result<Box<T>, Box<dyn Any>> {
    //let any: Box<dyn Any> = expr;
    //any.downcast::<T>()
    todo!()
}
