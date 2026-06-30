#![allow(missing_docs)]
mod uuid;

pub use uuid::Uuid;

#[derive(Default)]
pub struct SysRandom;

impl SysRandom {
    pub fn new() -> Self {
        Self
    }

    pub fn uuid_v4(&self) -> Uuid {
        Uuid::new_v4()
    }
}
