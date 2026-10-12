/// Fuzz helpers exercising update-delta apply/undo round-trips.
#[cfg(any(test, feature = "test", fuzzing))]
pub mod test_delta_apply;
