use crate::imp::random::Uuid;

pub type TaskID = u128;

pub fn new_task_id() -> TaskID {
    Uuid::new_v4().as_u128()
}
