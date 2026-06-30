use super::*;

#[cfg(target_os = "linux")]
pub(crate) enum FileFutureState<T> {
    Init,
    Pending(Arc<OpState<T>>),
    Done,
}
