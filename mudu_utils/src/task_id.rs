use uuid::Uuid;

/// TaskID use to store async task related context
/// Any async function can have a TaskID parameter to retrieve this task context
/// If rust can support [Custom Future contexts](https://github.com/rust-lang/rfcs/issues/2900)
/// The context information can be kept in Future
pub type TaskID = u128;

pub fn new_task_id() -> TaskID {
    Uuid::new_v4().as_u128()
}