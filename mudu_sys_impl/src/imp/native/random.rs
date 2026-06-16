use uuid::Uuid;

pub struct Random;

impl Random {
    pub fn uuid_v4() -> Uuid {
        Uuid::new_v4()
    }
}
