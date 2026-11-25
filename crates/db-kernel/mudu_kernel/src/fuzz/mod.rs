//! Fuzzing targets, harnesses, and golden-corpus helpers for the kernel.

#![allow(missing_docs)]

pub mod _fuzz_run;
mod _golden_corpus;
pub mod _test_target;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    #[test]
    fn golden_corpus_path_points_at_the_corpus_dir() {
        let path = super::_golden_corpus::golden_corpus_path();
        assert!(path.ends_with("golden_corpus"));
        assert!(std::path::PathBuf::from(&path).is_dir());
    }

    #[test]
    fn test_target_replays_placeholder_targets_without_panicking() {
        // `_type_convert`/`_x_log_append` are no-op placeholders.
        super::_test_target::_test::_test_target("_type_convert");
        super::_test_target::_test::_test_target("_x_log_append");
    }
}
