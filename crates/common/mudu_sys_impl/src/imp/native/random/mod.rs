mod uuid;

pub use uuid::Uuid;

/// Random generation subsystem - native implementation.
#[derive(Default)]
pub struct SysRandom;

impl SysRandom {
    /// Creates the random generation subsystem handle.
    pub fn new() -> Self {
        Self
    }

    /// Generates a random version-4 UUID.
    pub fn uuid_v4(&self) -> Uuid {
        Uuid::new_v4()
    }
}
