use crate::imp::random::Uuid;

/// Unique id of a task or thread, used as the key of the task context
/// registry.
pub type TaskID = u128;

/// Generates a fresh task id from a random UUID v4.
pub fn new_task_id() -> TaskID {
    Uuid::new_v4().as_u128()
}
