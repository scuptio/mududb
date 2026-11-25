//! Concrete language implementations and the shared language abstraction.

pub mod assemblyscript;
pub mod c;
pub mod csharp;
pub mod go;
pub mod lang;
pub use lang::name::{lang_non_scalar_name, lang_scalar_name};
pub mod python;
pub mod rust;
